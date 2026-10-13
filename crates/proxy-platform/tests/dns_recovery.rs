use anyhow::{Result, bail};
use proxy_platform::dns::{AdapterDns, DnsBackend, DnsController};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    servers: BTreeMap<String, String>,
    fail_id: Option<String>,
}
#[derive(Clone)]
struct MemoryDns(Arc<Mutex<State>>);
impl DnsBackend for MemoryDns {
    fn adapters(&self) -> Result<Vec<AdapterDns>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .servers
            .iter()
            .map(|(id, servers)| AdapterDns {
                id: id.clone(),
                name: id.clone(),
                servers: servers.clone(),
            })
            .collect())
    }
    fn read(&self, id: &str) -> Result<String> {
        Ok(self.0.lock().unwrap().servers[id].clone())
    }
    fn set(&self, id: &str, servers: &str) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.fail_id.as_deref() == Some(id) {
            bail!("接口暂时不可用");
        }
        state.servers.insert(id.into(), servers.into());
        Ok(())
    }
}
fn backend() -> MemoryDns {
    MemoryDns(Arc::new(Mutex::new(State {
        servers: BTreeMap::from([
            ("auto".into(), String::new()),
            ("manual".into(), "1.1.1.1,8.8.8.8".into()),
        ]),
        fail_id: None,
    })))
}

#[test]
fn a_new_controller_recovers_automatic_and_manual_settings_after_interruption() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("dns.json");
    let backend = backend();
    let baseline = backend.0.lock().unwrap().servers.clone();
    {
        let mut controller = DnsController::new(&path, backend.clone());
        controller.enable("127.0.0.2").unwrap();
        assert!(path.exists());
        assert!(controller.active());
        assert!(
            backend
                .0
                .lock()
                .unwrap()
                .servers
                .values()
                .all(|s| s == "127.0.0.2")
        );
    }
    let mut restarted = DnsController::new(&path, backend.clone());
    restarted.restore().unwrap();
    assert_eq!(backend.0.lock().unwrap().servers, baseline);
    assert!(!path.exists());
}

#[test]
fn partial_restore_keeps_failed_entries_for_the_next_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("dns.json");
    let backend = backend();
    let mut controller = DnsController::new(&path, backend.clone());
    controller.enable("127.0.0.2").unwrap();
    backend.0.lock().unwrap().fail_id = Some("manual".into());
    assert!(controller.restore().is_err());
    let remaining: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(remaining["adapters"].as_array().unwrap().len(), 1);
    assert_eq!(remaining["adapters"][0]["id"], "manual");
    assert_eq!(backend.read("auto").unwrap(), "");
    backend.0.lock().unwrap().fail_id = None;
    controller.restore().unwrap();
    assert_eq!(backend.read("manual").unwrap(), "1.1.1.1,8.8.8.8");
}

#[test]
fn journal_write_failure_prevents_any_system_change() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("file");
    std::fs::write(&path, "occupied").unwrap();
    let backend = backend();
    let baseline = backend.0.lock().unwrap().servers.clone();
    let mut controller = DnsController::new(path.join("dns.json"), backend.clone());
    assert!(controller.enable("127.0.0.2").is_err());
    assert_eq!(backend.0.lock().unwrap().servers, baseline);
}
