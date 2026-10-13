use crate::state::Shared;
use anyhow::{Context, Result, ensure};
use proxy_core::{
    config::Protocol,
    model::{ProcessIdentity, ReconnectResult},
};
use proxy_platform::{connections, process};
use std::{
    collections::{HashMap, HashSet},
    net::ToSocketAddrs,
    sync::atomic::Ordering,
};

pub(crate) fn reconnect(
    shared: &Shared,
    identities: &[ProcessIdentity],
    group_id: Option<u32>,
) -> Result<ReconnectResult> {
    ensure!(!shared.stopping.load(Ordering::Acquire), "请先启动代理引擎");
    ensure!(identities.len() <= 8192, "一次最多处理 8192 个进程");
    let mut result = ReconnectResult::default();
    if identities.is_empty() {
        return Ok(result);
    }
    let config = shared.state.lock().unwrap().config.clone();
    let mut endpoints = HashSet::new();
    for group in &config.proxy_groups {
        endpoints.extend(
            (group.host.as_str(), group.port)
                .to_socket_addrs()
                .with_context(|| format!("无法确认代理组 {} 的地址，未断开连接", group.name))?
                .filter_map(|address| match address {
                    std::net::SocketAddr::V4(address) => Some(address),
                    _ => None,
                }),
        );
    }
    let candidates = connections::tcp_snapshot()?;
    // Protect wildcard/hostname/local-IP proxy listeners without relying on adapter discovery.
    let mut protected: HashSet<_> = candidates
        .iter()
        .filter(|c| {
            c.is_listener()
                && config
                    .proxy_groups
                    .iter()
                    .any(|group| group.port == c.key.local.port())
        })
        .map(|c| c.pid)
        .collect();
    protected.extend(
        connections::tcp6_listener_owners()?
            .into_iter()
            .filter(|(_, port)| config.proxy_groups.iter().any(|group| group.port == *port))
            .map(|(pid, _)| pid),
    );
    let selected: HashMap<_, _> = identities.iter().map(|id| (id.pid, id)).collect();
    let mut verified = HashSet::new();
    for identity in selected.values() {
        if identity.pid == std::process::id() || protected.contains(&identity.pid) {
            continue;
        }
        match process::query_ancestors(identity.pid) {
            Ok(ancestors) if ancestors.last().is_some_and(|p| p.identity == **identity) => {
                let mut state = shared.state.lock().unwrap();
                for process in ancestors {
                    state.tree.upsert(process);
                }
                verified.insert(identity.pid);
            }
            Ok(_) => {}
            Err(error) => result
                .failures
                .push(format!("PID {}：无法核对进程身份：{error:#}", identity.pid)),
        }
    }
    // Enumerate once: application retries cannot be swept into this same operation.
    for connection in candidates
        .iter()
        .filter(|c| c.is_established() && selected.contains_key(&c.pid))
    {
        let identity = selected[&connection.pid];
        if !verified.contains(&connection.pid)
            || connection.key.remote.ip().is_loopback()
            || endpoints.contains(&connection.key.remote)
        {
            result.skipped += 1;
            continue;
        }
        let eligible = {
            let state = shared.state.lock().unwrap();
            if shared.stopping.load(Ordering::Acquire) || state.config != config {
                result
                    .failures
                    .push("代理状态或配置已变化，剩余连接未处理".into());
                break;
            }
            state.route(identity, Protocol::Tcp).is_some_and(|route| {
                group_id.is_none_or(|group| route.group_id == group)
                    && state.policy.allows_destination(
                        &route,
                        *connection.key.remote.ip(),
                        connection.key.remote.port(),
                    )
            }) && !state.reflections.contains_key(&connection.key)
        };
        if !eligible {
            result.skipped += 1;
            continue;
        }
        match connections::disconnect_tcp(connection, identity) {
            Ok(true) => result.disconnected += 1,
            Ok(false) => result.skipped += 1,
            Err(error) => result.failures.push(format!(
                "PID {} · {} → {}：{error:#}",
                connection.pid, connection.key.local, connection.key.remote
            )),
        }
    }
    Ok(result)
}
