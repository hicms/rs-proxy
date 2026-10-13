use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::Ipv4Addr};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
    #[default]
    Both,
}

impl Protocol {
    pub fn allows(self, transport: Self) -> bool {
        self == Self::Both || self == transport
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct ProxyGroup {
    pub id: u32,
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl Default for ProxyGroup {
    fn default() -> Self {
        Self {
            id: 0,
            name: "默认代理".into(),
            host: "127.0.0.1".into(),
            port: 7890,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct TrafficFilter {
    pub include_cidrs: Vec<String>,
    pub exclude_cidrs: Vec<String>,
    pub include_ports: Vec<String>,
    pub exclude_ports: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct AutoRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub process_name: String,
    pub cmdline_pattern: String,
    pub image_path_pattern: String,
    pub proxy_group_id: u32,
    pub protocol: Protocol,
    pub dst_filter: TrafficFilter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct DnsConfig {
    pub enabled: bool,
    pub upstream_host: String,
    pub upstream_port: u16,
    pub listen_host: String,
    pub listen_port: u16,
    pub proxy_group_id: u32,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            upstream_host: "8.8.8.8".into(),
            upstream_port: 53,
            listen_host: "127.0.0.2".into(),
            listen_port: 53,
            proxy_group_id: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub close_to_tray: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            close_to_tray: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct SynParkingConfig {
    pub watchdog_ms: u32,
    pub pool_size: usize,
}

impl Default for SynParkingConfig {
    fn default() -> Self {
        Self {
            watchdog_ms: 20,
            pool_size: 256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub revision: u32,
    pub proxy_groups: Vec<ProxyGroup>,
    pub auto_rules: Vec<AutoRule>,
    pub default_exclude_cidrs: Vec<String>,
    pub proxy_process_dns: bool,
    pub dns: DnsConfig,
    pub ui: UiConfig,
    pub tcp_syn_parking: SynParkingConfig,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            proxy_groups: vec![ProxyGroup::default()],
            auto_rules: vec![],
            default_exclude_cidrs: [
                "127.0.0.0/8",
                "10.0.0.0/8",
                "172.16.0.0/12",
                "192.168.0.0/16",
                "169.254.0.0/16",
            ]
            .map(String::from)
            .to_vec(),
            dns: DnsConfig::default(),
            proxy_process_dns: true,
            ui: UiConfig::default(),
            tcp_syn_parking: SynParkingConfig::default(),
            log_level: "info".into(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let fail = |s: &str| Err(ConfigError(s.into()));
        if self.version != 1 {
            return fail("不支持的配置版本");
        }
        if self.proxy_groups.is_empty() {
            return fail("至少需要一个代理组");
        }
        let mut groups = HashSet::new();
        for group in &self.proxy_groups {
            if !groups.insert(group.id) {
                return fail("代理组 ID 重复");
            }
            if group.name.trim().is_empty() || group.host.trim().is_empty() || group.port == 0 {
                return fail("代理组需要名称、主机和有效端口");
            }
            if group.host.contains(['/', '\\', '\r', '\n', ' ']) {
                return fail("代理主机格式无效");
            }
        }
        let mut ids = HashSet::new();
        for rule in &self.auto_rules {
            if rule.id.trim().is_empty() || !ids.insert(&rule.id) {
                return fail("规则 ID 为空或重复");
            }
            if rule.name.trim().is_empty() {
                return fail("规则名称不能为空");
            }
            if rule.process_name.trim().is_empty()
                && rule.image_path_pattern.trim().is_empty()
                && rule.cmdline_pattern.trim().is_empty()
            {
                return fail("规则至少需要一个进程匹配条件");
            }
            if !groups.contains(&rule.proxy_group_id) {
                return fail("规则引用的代理组不存在");
            }
            crate::filter::CompiledFilter::new(&rule.dst_filter)?;
        }
        crate::filter::parse_networks(&self.default_exclude_cidrs)?;
        let dns = &self.dns;
        if !groups.contains(&dns.proxy_group_id) {
            return fail("DNS 引用的代理组不存在");
        }
        if dns.upstream_host.parse::<Ipv4Addr>().is_err() || dns.upstream_port == 0 {
            return fail("DNS 上游需要 IPv4 地址和有效端口");
        }
        if !dns
            .listen_host
            .parse::<Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
            || dns.listen_port != 53
        {
            return fail("系统 DNS 转发必须监听回环地址的 53 端口");
        }
        if !matches!(self.ui.theme.as_str(), "system" | "light" | "dark") {
            return fail("无效的主题");
        }
        if !matches!(
            self.log_level.as_str(),
            "trace" | "debug" | "info" | "warn" | "error"
        ) {
            return fail("无效的日志级别");
        }
        if !(5..=50).contains(&self.tcp_syn_parking.watchdog_ms)
            || !(32..=4096).contains(&self.tcp_syn_parking.pool_size)
        {
            return fail("SYN 等待时间必须为 5–50ms，容量为 32–4096");
        }
        Ok(())
    }
}
