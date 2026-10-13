use crate::{packet::Packet, relay::UdpPacket, state::Shared};
use proxy_platform::windivert::{Address, DivertHandle};
use std::{
    net::SocketAddrV4,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Sender;

struct Pending {
    bytes: Vec<u8>,
    address: Address,
    packet: Packet,
    deadline: Instant,
}
pub(crate) struct Capture {
    pub shared: Arc<Shared>,
    pub network: Arc<DivertHandle>,
    pub tcp_port: u16,
    pub udp: Sender<UdpPacket>,
    pending: Mutex<Vec<Pending>>,
    pending_count: AtomicUsize,
    receiver_done: AtomicBool,
}
impl Capture {
    pub fn new(
        shared: Arc<Shared>,
        network: Arc<DivertHandle>,
        tcp_port: u16,
        udp: Sender<UdpPacket>,
    ) -> Self {
        Self {
            shared,
            network,
            tcp_port,
            udp,
            pending: Mutex::new(Vec::new()),
            pending_count: AtomicUsize::new(0),
            receiver_done: AtomicBool::new(false),
        }
    }
    fn inject(&self, mut bytes: Vec<u8>, mut address: Address, proxied: bool) {
        let result = self
            .network
            .recalculate(&mut bytes, &mut address)
            .and_then(|_| self.network.send(&bytes, &address).map_err(Into::into));
        if let Err(error) = result {
            self.shared.error(error);
        } else {
            (if proxied {
                &self.shared.counters.proxied
            } else {
                &self.shared.counters.direct
            })
            .fetch_add(1, Ordering::Relaxed);
        }
    }
    fn process(&self, mut pending: Pending, expired: bool) -> Option<Pending> {
        let packet = &pending.packet;
        let stopping = self.shared.stopping.load(Ordering::Acquire);
        if packet.protocol == 17 {
            let route = self
                .shared
                .state
                .lock()
                .unwrap()
                .udp_route(&packet.key, pending.address.timestamp);
            match route {
                Ok(Some(Some(route))) if !stopping => {
                    let payload = pending.bytes[packet.payload..packet.length].to_vec();
                    if self
                        .udp
                        .try_send(UdpPacket {
                            route,
                            local: packet.key.local,
                            remote: packet.key.remote,
                            payload,
                            address: pending.address,
                        })
                        .is_err()
                    {
                        self.shared.error("UDP 转发队列已满，已丢弃该代理数据报");
                    } else {
                        self.shared.counters.proxied.fetch_add(1, Ordering::Relaxed);
                    }
                    return None;
                }
                Ok(None) if !expired && !stopping => return Some(pending),
                Err(_) => return None,
                _ => {
                    self.inject(pending.bytes, pending.address, false);
                    return None;
                }
            }
        }
        let mut state = self.shared.state.lock().unwrap();
        if packet.key.local.port() == self.tcp_port {
            let reverse = (
                *packet.key.local.ip(),
                *packet.key.remote.ip(),
                packet.key.remote.port(),
            );
            if let Some(entry) = state.reverse.get(&reverse) {
                packet.rewrite(&mut pending.bytes, entry.key.remote, entry.key.local);
                pending.address = entry.address;
                pending.address.set_inbound();
                drop(state);
                self.inject(pending.bytes, pending.address, true);
                return None;
            }
        }
        let reflection = if packet.initial_syn() && !stopping {
            let ttl = state.ttl();
            match state
                .flows
                .on_syn(&packet.key, packet.sequence, pending.address.timestamp, ttl)
            {
                Some(Some(route)) => {
                    let reflection = state.reflect(&packet.key, &route, pending.address);
                    if reflection.is_none() {
                        self.shared.error("TCP 代理映射无法分配，连接已阻止");
                        return None;
                    }
                    reflection
                }
                Some(None) => {
                    state.detach(&packet.key);
                    None
                }
                None if !expired => return Some(pending),
                None => {
                    state.detach(&packet.key);
                    state.flows.abandon(
                        packet.key.clone(),
                        packet.sequence,
                        pending.address.timestamp,
                    );
                    self.shared
                        .counters
                        .timed_out
                        .fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        } else {
            state.reflections.get(&packet.key).cloned()
        };
        drop(state);
        if let Some(reflection) = reflection {
            let source = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, reflection.alias);
            let destination = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, self.tcp_port);
            packet.rewrite(&mut pending.bytes, source, destination);
            pending.address.set_loopback_outbound();
            self.inject(pending.bytes, pending.address, true);
        } else {
            self.inject(pending.bytes, pending.address, false);
        }
        None
    }
    pub fn receive_loop(&self) {
        let mut buffer = vec![0u8; 65535];
        loop {
            let (length, address) = match self.network.receive(&mut buffer) {
                Ok(packet) => packet,
                Err(error) => {
                    if !self.shared.stopping.load(Ordering::Acquire) {
                        self.shared.error(format!("抓包线程结束：{error}"));
                    }
                    self.shared.stopping.store(true, Ordering::Release);
                    let _ = self.network.shutdown_receive();
                    break;
                }
            };
            let bytes = buffer[..length].to_vec();
            let Some(packet) = Packet::parse(&bytes) else {
                self.inject(bytes, address, false);
                continue;
            };
            let (duration, capacity) = {
                let state = self.shared.state.lock().unwrap();
                (
                    state.config.tcp_syn_parking.watchdog_ms,
                    state.config.tcp_syn_parking.pool_size,
                )
            };
            let pending = Pending {
                bytes,
                address,
                packet,
                deadline: Instant::now() + Duration::from_millis(duration as u64),
            };
            if let Some(pending) = self.process(pending, false) {
                if self
                    .pending_count
                    .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |count| {
                        (count < capacity).then_some(count + 1)
                    })
                    .is_ok()
                {
                    if pending.packet.initial_syn() {
                        self.shared.counters.parked.fetch_add(1, Ordering::Relaxed);
                    }
                    self.pending.lock().unwrap().push(pending);
                } else {
                    self.process(pending, true);
                }
            }
        }
        self.receiver_done.store(true, Ordering::Release);
    }
    pub fn watchdog_loop(&self) {
        let mut last_collect = Instant::now();
        loop {
            if last_collect.elapsed() >= Duration::from_secs(1) {
                self.shared.state.lock().unwrap().collect();
                last_collect = Instant::now();
            }
            let stopping = self.shared.stopping.load(Ordering::Acquire);
            let queue = std::mem::take(&mut *self.pending.lock().unwrap());
            let mut remaining = Vec::new();
            for pending in queue {
                let expired = stopping || Instant::now() >= pending.deadline;
                if let Some(pending) = self.process(pending, expired) {
                    remaining.push(pending);
                } else {
                    self.pending_count.fetch_sub(1, Ordering::AcqRel);
                }
            }
            self.pending.lock().unwrap().extend(remaining);
            if stopping
                && self.receiver_done.load(Ordering::Acquire)
                && self.pending_count.load(Ordering::Acquire) == 0
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
