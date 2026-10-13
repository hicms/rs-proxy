use crate::{
    config::{AutoRule, Config, ConfigError, Protocol},
    filter::{CompiledFilter, parse_networks},
    model::{ProcessInfo, Route},
};
use ipnet::Ipv4Net;
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destination {
    pub address: SocketAddrV4,
    pub dns: bool,
}

pub fn wildcard_match(pattern: &str, input: &str) -> bool {
    let pattern: Vec<_> = pattern.to_lowercase().chars().collect();
    let input: Vec<_> = input.to_lowercase().chars().collect();
    let (mut p, mut i, mut star, mut retry) = (0, 0, None, 0);
    while i < input.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == input[i]) {
            p += 1;
            i += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = i;
        } else if let Some(last) = star {
            p = last + 1;
            retry += 1;
            i = retry;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

pub fn cmdline_match(pattern: &str, cmdline: &str) -> bool {
    if pattern.contains(['*', '?']) {
        wildcard_match(pattern, cmdline)
    } else {
        let input = cmdline.to_lowercase();
        pattern
            .to_lowercase()
            .split_whitespace()
            .all(|part| input.contains(part))
    }
}

fn matches_process(rule: &AutoRule, process: &ProcessInfo) -> bool {
    (rule.process_name.is_empty() || wildcard_match(&rule.process_name, &process.name))
        && (rule.cmdline_pattern.is_empty()
            || cmdline_match(&rule.cmdline_pattern, &process.cmdline))
        && (rule.image_path_pattern.is_empty()
            || process
                .image_path
                .to_lowercase()
                .starts_with(&rule.image_path_pattern.replace('/', "\\").to_lowercase()))
}

#[derive(Clone, Debug)]
pub struct RoutingPolicy {
    rules: Vec<(AutoRule, CompiledFilter)>,
    excludes: Vec<Ipv4Net>,
    dns: Option<SocketAddrV4>,
}

impl RoutingPolicy {
    pub fn new(config: &Config) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            rules: config
                .auto_rules
                .iter()
                .map(|rule| Ok((rule.clone(), CompiledFilter::new(&rule.dst_filter)?)))
                .collect::<Result<_, ConfigError>>()?,
            excludes: parse_networks(&config.default_exclude_cidrs)?,
            dns: config.proxy_process_dns.then(|| {
                SocketAddrV4::new(
                    config.dns.upstream_host.parse().unwrap(),
                    config.dns.upstream_port,
                )
            }),
        })
    }

    pub fn classify(&self, ancestors: &[&ProcessInfo], protocol: Protocol) -> Option<Route> {
        self.rules.iter().find_map(|(rule, _)| {
            (rule.enabled
                && (protocol == Protocol::Both || rule.protocol.allows(protocol))
                && ancestors
                    .iter()
                    .any(|process| matches_process(rule, process)))
            .then(|| Route {
                group_id: rule.proxy_group_id,
                rule_id: Some(rule.id.clone()),
                source: "rule".into(),
            })
        })
    }

    pub fn allows_destination(&self, route: &Route, ip: Ipv4Addr, port: u16) -> bool {
        self.destination(route, ip, port).is_some()
    }

    pub fn destination(&self, route: &Route, ip: Ipv4Addr, port: u16) -> Option<Destination> {
        let allowed = match &route.rule_id {
            None => true,
            Some(id) => self
                .rules
                .iter()
                .find(|(rule, _)| &rule.id == id)
                .is_some_and(|(_, filter)| filter.allows(ip, port)),
        };
        if !allowed || ip.is_loopback() {
            return None;
        }
        // Selected processes may use a LAN resolver; only DNS bypasses global direct CIDRs.
        if port == 53
            && let Some(address) = self.dns
        {
            return Some(Destination { address, dns: true });
        }
        (!self.excludes.iter().any(|net| net.contains(&ip))).then_some(Destination {
            address: SocketAddrV4::new(ip, port),
            dns: false,
        })
    }
}
