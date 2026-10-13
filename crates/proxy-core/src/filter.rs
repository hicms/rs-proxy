use crate::config::{ConfigError, TrafficFilter};
use ipnet::Ipv4Net;
use std::{net::Ipv4Addr, ops::RangeInclusive};

pub fn parse_networks(values: &[String]) -> Result<Vec<Ipv4Net>, ConfigError> {
    values
        .iter()
        .map(|s| {
            let text = if s.contains('/') {
                s.clone()
            } else {
                format!("{s}/32")
            };
            text.parse()
                .map_err(|_| ConfigError(format!("无效的 IPv4/CIDR：{s}")))
        })
        .collect()
}

fn parse_ports(values: &[String]) -> Result<Vec<RangeInclusive<u16>>, ConfigError> {
    values
        .iter()
        .map(|s| {
            let (start, end) = s.split_once('-').unwrap_or((s, s));
            match (start.parse::<u16>(), end.parse::<u16>()) {
                (Ok(a), Ok(b)) if a > 0 && a <= b => Ok(a..=b),
                _ => Err(ConfigError(format!("无效的端口范围：{s}"))),
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct CompiledFilter {
    include: Vec<Ipv4Net>,
    exclude: Vec<Ipv4Net>,
    include_ports: Vec<RangeInclusive<u16>>,
    exclude_ports: Vec<RangeInclusive<u16>>,
}

impl CompiledFilter {
    pub fn new(filter: &TrafficFilter) -> Result<Self, ConfigError> {
        Ok(Self {
            include: parse_networks(&filter.include_cidrs)?,
            exclude: parse_networks(&filter.exclude_cidrs)?,
            include_ports: parse_ports(&filter.include_ports)?,
            exclude_ports: parse_ports(&filter.exclude_ports)?,
        })
    }

    pub fn allows(&self, ip: Ipv4Addr, port: u16) -> bool {
        !self.exclude.iter().any(|net| net.contains(&ip))
            && !self.exclude_ports.iter().any(|range| range.contains(&port))
            && (self.include.is_empty() || self.include.iter().any(|net| net.contains(&ip)))
            && (self.include_ports.is_empty()
                || self.include_ports.iter().any(|range| range.contains(&port)))
    }
}
