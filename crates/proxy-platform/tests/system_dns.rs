use proxy_platform::dns::{DnsBackend, DnsController, WindowsDns};

#[test]
#[ignore = "temporarily changes active adapter DNS; run explicitly with RS_PROXY_SYSTEM_DNS_TEST=1"]
fn active_adapter_dns_is_applied_and_restored_exactly() {
    assert_eq!(
        std::env::var("RS_PROXY_SYSTEM_DNS_TEST").as_deref(),
        Ok("1")
    );
    let directory = std::env::temp_dir().join("rs-proxy-dns-verification");
    std::fs::create_dir_all(&directory).unwrap();
    let journal = directory.join("dns-recovery.json");
    let mut controller = DnsController::new(&journal, WindowsDns);
    controller
        .restore()
        .expect("finish any previous verification recovery first");
    let before = WindowsDns.adapters().unwrap();
    assert!(!before.is_empty());
    std::fs::write(
        directory.join("before.json"),
        serde_json::to_vec_pretty(&before).unwrap(),
    )
    .unwrap();
    let applied = controller.enable("127.0.0.2");
    let observed: Vec<_> = before
        .iter()
        .map(|adapter| WindowsDns.read(&adapter.id))
        .collect();
    let restored = controller.restore();
    // All assertions happen after restoration has been attempted.
    restored.expect("DNS restore failed; recovery journal remains at the printed TEMP path");
    applied.unwrap();
    for (adapter, current) in before.iter().zip(observed) {
        assert_eq!(current.unwrap(), "127.0.0.2");
        assert_eq!(WindowsDns.read(&adapter.id).unwrap(), adapter.servers);
    }
    assert!(!journal.exists());
    println!(
        "Applied and restored {} active IPv4 adapters; baseline: {}",
        before.len(),
        directory.display()
    );
}
