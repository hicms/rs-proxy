use crate::config::{Config, Protocol};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub sequence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ProcessIcon {
    pub identity: ProcessIdentity,
    pub data_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ProcessInfo {
    pub identity: ProcessIdentity,
    pub parent: Option<ProcessIdentity>,
    pub name: String,
    pub image_path: String,
    pub cmdline: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Route {
    pub group_id: u32,
    pub rule_id: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ProcessView {
    pub process: ProcessInfo,
    pub route: Option<Route>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Connection {
    pub pid: u32,
    pub protocol: Protocol,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: String,
    pub remote_port: u16,
    pub state: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct EngineStats {
    pub running: bool,
    pub elevated: bool,
    pub tcp_connections: u32,
    pub udp_sessions: u32,
    pub proxied_packets: f64,
    pub direct_packets: f64,
    pub uploaded_bytes: f64,
    pub downloaded_bytes: f64,
    pub parked_syns: f64,
    pub timed_out_syns: f64,
    pub last_error: Option<String>,
    pub dns_active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct TrafficStats {
    pub uploaded_bytes: f64,
    pub downloaded_bytes: f64,
    pub upload_bytes_per_second: f64,
    pub download_bytes_per_second: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct GroupTraffic {
    pub group_id: u32,
    pub stats: TrafficStats,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ProcessTraffic {
    pub identity: ProcessIdentity,
    pub total: TrafficStats,
    pub groups: Vec<GroupTraffic>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct TrafficSnapshot {
    pub total: TrafficStats,
    pub groups: Vec<GroupTraffic>,
    pub processes: Vec<ProcessTraffic>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Snapshot {
    pub config: Config,
    pub processes: Vec<ProcessView>,
    pub connections: Vec<Connection>,
    pub stats: EngineStats,
    pub traffic: TrafficSnapshot,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct AutostartState {
    pub enabled: bool,
    pub start_minimized: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ProxyTestResult {
    pub latency_ms: u32,
    pub message: String,
}
