use crate::traffic::{Traffic, TrafficGroup};
use anyhow::{Result, bail};
use proxy_core::{
    config::{Config, Protocol, ProxyGroup},
    flow::{FlowKey, FlowTable},
    model::{EngineStats, ProcessIdentity, Route},
    rules::{Destination, RoutingPolicy},
    tree::ProcessTree,
    udp::{UdpLookup, UdpOwner, UdpTable},
};
use proxy_platform::{
    process,
    windivert::{SocketEvent, frequency},
};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};

#[derive(Default)]
pub(crate) struct Counters {
    pub tcp: AtomicU32,
    pub udp: AtomicU32,
    pub proxied: AtomicU64,
    pub direct: AtomicU64,
    pub parked: AtomicU64,
    pub timed_out: AtomicU64,
}
pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub traffic: Arc<Traffic>,
    pub counters: Counters,
    pub stopping: AtomicBool,
    pub last_error: Mutex<Option<String>>,
}
impl Shared {
    pub fn error(&self, error: impl std::fmt::Display) {
        let message = error.to_string();
        tracing::error!(%message);
        *self.last_error.lock().unwrap() = Some(message);
    }
    pub fn stats(&self) -> EngineStats {
        let c = &self.counters;
        let traffic = self.traffic.total();
        EngineStats {
            running: !self.stopping.load(Ordering::Acquire),
            elevated: proxy_platform::is_elevated(),
            tcp_connections: c.tcp.load(Ordering::Relaxed),
            udp_sessions: c.udp.load(Ordering::Relaxed),
            proxied_packets: c.proxied.load(Ordering::Relaxed) as f64,
            direct_packets: c.direct.load(Ordering::Relaxed) as f64,
            uploaded_bytes: traffic.uploaded_bytes,
            downloaded_bytes: traffic.downloaded_bytes,
            parked_syns: c.parked.load(Ordering::Relaxed) as f64,
            timed_out_syns: c.timed_out.load(Ordering::Relaxed) as f64,
            last_error: self.last_error.lock().unwrap().clone(),
            dns_active: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Reflection {
    pub address: proxy_platform::windivert::Address,
    pub key: FlowKey,
    pub alias: u16,
    pub timestamp: i64,
    pub group: ProxyGroup,
    pub identity: ProcessIdentity,
    pub traffic_group: TrafficGroup,
    pub destination: Destination,
}
pub(crate) type ReverseKey = (Ipv4Addr, Ipv4Addr, u16);
#[derive(Clone, Debug)]
pub(crate) struct ProxyRoute {
    pub group: ProxyGroup,
    pub destination: Destination,
    pub traffic_group: TrafficGroup,
}
#[derive(Clone)]
pub(crate) struct UdpRoute {
    pub owner: UdpOwner,
    pub group: ProxyGroup,
    pub traffic_group: TrafficGroup,
    pub destination: Destination,
}

pub(crate) struct State {
    pub traffic: Arc<Traffic>,
    traffic_groups: HashMap<u32, TrafficGroup>,
    pub config: Config,
    pub policy: RoutingPolicy,
    pub tree: ProcessTree,
    pub flows: FlowTable<ProxyRoute>,
    pub reflections: HashMap<FlowKey, Reflection>,
    pub reverse: HashMap<ReverseKey, Reflection>,
    expirations: HashMap<u16, std::time::Instant>,
    udp: UdpTable,
    next_alias: u16,
}
impl State {
    pub fn new(config: Config) -> Result<Self> {
        let policy = RoutingPolicy::new(&config)?;
        let mut tree = ProcessTree::default();
        tree.synchronize(process::snapshot()?);
        let traffic = Arc::new(Traffic::new(std::time::Instant::now()));
        let traffic_groups = traffic.sync_groups(&config.proxy_groups);
        Ok(Self {
            traffic,
            traffic_groups,
            config,
            policy,
            tree,
            flows: FlowTable::default(),
            reflections: HashMap::new(),
            reverse: HashMap::new(),
            expirations: HashMap::new(),
            udp: UdpTable::default(),
            next_alias: 1024,
        })
    }
    pub fn validate_update(&self, config: &Config, replacement: Option<(u32, u32)>) -> Result<()> {
        config.validate()?;
        if let Some((from, to)) = replacement
            && (from == to || !config.proxy_groups.iter().any(|g| g.id == to))
        {
            bail!("替换代理组无效");
        }
        for group in &self.config.proxy_groups {
            if !config.proxy_groups.iter().any(|next| next.id == group.id)
                && self.tree.uses_group(group.id)
                && replacement.is_none_or(|(from, _)| from != group.id)
            {
                bail!("代理组仍被手动代理使用，请先重新分配引用");
            }
        }
        Ok(())
    }
    pub fn update(&mut self, config: Config, replacement: Option<(u32, u32)>) -> Result<()> {
        self.validate_update(&config, replacement)?;
        self.policy = RoutingPolicy::new(&config)?;
        if let Some((from, to)) = replacement {
            self.tree.reassign_group(from, to);
        }
        self.traffic_groups = self.traffic.sync_groups(&config.proxy_groups);
        self.config = config;
        Ok(())
    }
    pub fn override_process(
        &mut self,
        identity: &ProcessIdentity,
        group: Option<u32>,
    ) -> Result<()> {
        if let Some(group) = group
            && !self.config.proxy_groups.iter().any(|g| g.id == group)
        {
            bail!("代理组不存在");
        }
        let ancestors = process::query_ancestors(identity.pid)?;
        if ancestors
            .last()
            .is_none_or(|process| &process.identity != identity)
        {
            bail!("进程身份已变化，请刷新后重试");
        }
        for process in ancestors {
            self.tree.upsert(process);
        }
        if !self.tree.set_manual(identity.pid, group) {
            bail!("进程已退出");
        }
        Ok(())
    }
    pub(crate) fn route(&self, identity: &ProcessIdentity, protocol: Protocol) -> Option<Route> {
        if identity.pid == std::process::id() {
            return None;
        }
        self.tree.route(identity, protocol, &self.policy)
    }
    fn proxy_route(
        &self,
        identity: &ProcessIdentity,
        protocol: Protocol,
        remote: SocketAddrV4,
    ) -> Option<ProxyRoute> {
        let route = self.route(identity, protocol)?;
        let destination = self
            .policy
            .destination(&route, *remote.ip(), remote.port())?;
        let group = self
            .config
            .proxy_groups
            .iter()
            .find(|group| group.id == route.group_id)?
            .clone();
        Some(ProxyRoute {
            traffic_group: self.traffic_groups.get(&group.id)?.clone(),
            group,
            destination,
        })
    }
    pub fn socket_event(
        &mut self,
        event: SocketEvent,
        ancestors: Vec<proxy_core::model::ProcessInfo>,
    ) {
        let identity = ancestors.last().map(|process| process.identity.clone());
        for process in ancestors {
            self.tree.upsert(process);
        }
        let key = FlowKey {
            local: event.local,
            remote: event.remote,
        };
        if event.protocol == 6 {
            if event.kind == 4 {
                let route = identity
                    .as_ref()
                    .and_then(|id| self.proxy_route(id, Protocol::Tcp, key.remote));
                self.flows
                    .publish(key, event.endpoint, event.timestamp, route, identity);
            } else if event.kind == 7 {
                self.flows.close(&key, event.endpoint);
            }
            return;
        }
        if event.protocol != 17 {
            return;
        }
        if event.kind == 7 {
            self.udp
                .close(event.endpoint, event.pid, event.local, event.timestamp);
        } else if matches!(event.kind, 3 | 4)
            && let Some(identity) = identity
        {
            self.udp.bind(
                UdpOwner {
                    endpoint: event.endpoint,
                    identity,
                    timestamp: event.timestamp,
                    local: event.local,
                },
                false,
                (event.kind == 4).then_some(event.remote),
            );
        }
    }
    pub fn seed_udp(&mut self, connections: &[proxy_core::model::Connection]) {
        let mut next = 1u64 << 63;
        for connection in connections.iter().filter(|c| c.protocol == Protocol::Udp) {
            if let Some(identity) = self.tree.identity(connection.pid).cloned()
                && let Ok(ip) = connection.local_address.parse()
            {
                self.udp.bind(
                    UdpOwner {
                        endpoint: next,
                        identity,
                        timestamp: 0,
                        local: SocketAddrV4::new(ip, connection.local_port),
                    },
                    true,
                    None,
                );
                next += 1;
            }
        }
    }
    pub fn udp_route(&self, key: &FlowKey, timestamp: i64) -> Result<Option<Option<UdpRoute>>> {
        let owner = match self.udp.lookup(key, timestamp) {
            UdpLookup::Pending => return Ok(None),
            UdpLookup::Stale => bail!("UDP 数据报所属 socket 已关闭或存在歧义"),
            UdpLookup::Found(owner) => owner,
        };
        Ok(Some(
            self.proxy_route(&owner.identity, Protocol::Udp, key.remote)
                .map(|route| UdpRoute {
                    owner: owner.clone(),
                    group: route.group,
                    destination: route.destination,
                    traffic_group: route.traffic_group,
                }),
        ))
    }
    pub fn udp_active(&self, owner: &UdpOwner) -> bool {
        self.udp.active(owner)
    }
    pub fn reflect(
        &mut self,
        key: &FlowKey,
        route: &ProxyRoute,
        address: proxy_platform::windivert::Address,
    ) -> Option<Reflection> {
        let decision = self.flows.get(key)?;
        let timestamp = decision.timestamp;
        let identity = decision.identity.clone()?;
        if let Some(existing) = self.reflections.get(key)
            && existing.timestamp == timestamp
        {
            return Some(existing.clone());
        }
        self.detach(key);
        let group = route.group.clone();
        let traffic_group = route.traffic_group.clone();
        for _ in 1024..=u16::MAX {
            let alias = self.next_alias;
            self.next_alias = if alias == u16::MAX { 1024 } else { alias + 1 };
            let reverse = (Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, alias);
            if self.reverse.contains_key(&reverse) {
                continue;
            }
            let reflection = Reflection {
                address,
                key: key.clone(),
                alias,
                timestamp,
                group,
                identity,
                traffic_group,
                destination: route.destination,
            };
            self.reverse.insert(reverse, reflection.clone());
            self.expirations.insert(
                alias,
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            );
            self.reflections.insert(key.clone(), reflection.clone());
            return Some(reflection);
        }
        None
    }
    pub fn accepted(&mut self, entry: &Reflection) {
        self.expirations.remove(&entry.alias);
    }
    pub fn finish(&mut self, entry: &Reflection) {
        self.expirations.insert(
            entry.alias,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        );
    }
    pub fn detach(&mut self, key: &FlowKey) {
        if let Some(old) = self.reflections.remove(key) {
            self.finish(&old);
        }
    }
    pub fn collect(&mut self) {
        self.flows
            .collect_unowned(proxy_platform::windivert::timestamp() - frequency() * 600);
        let now = std::time::Instant::now();
        let expired: Vec<_> = self
            .expirations
            .iter()
            .filter(|(_, end)| **end <= now)
            .map(|(alias, _)| *alias)
            .collect();
        for alias in expired {
            self.expirations.remove(&alias);
            if let Some(entry) =
                self.reverse
                    .remove(&(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, alias))
            {
                self.flows.release(&entry.key, entry.timestamp);
                if self
                    .reflections
                    .get(&entry.key)
                    .is_some_and(|current| current.timestamp == entry.timestamp)
                {
                    self.reflections.remove(&entry.key);
                }
            }
        }
        self.udp
            .collect(proxy_platform::windivert::timestamp() - frequency() * 30);
    }
    pub fn ttl(&self) -> i64 {
        frequency() * self.config.tcp_syn_parking.watchdog_ms as i64 / 1000
    }
}
