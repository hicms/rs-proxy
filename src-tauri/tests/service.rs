use rs_proxy::service::Service;
#[cfg(feature = "platform-tests")]
use std::path::Path;
#[cfg(feature = "platform-tests")]
static ENGINE_TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(feature = "platform-tests")]
#[tokio::test(flavor = "multi_thread")]
async fn service_persists_valid_configuration_and_releases_the_engine() {
    let _serial = ENGINE_TEST.lock().await;
    let data = tempfile::tempdir().unwrap();
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/windivert");
    let mut service = Service::open(data.path(), driver.clone()).unwrap();
    let mut config = service.config().clone();
    config.ui.theme = "dark".into();
    let saved = service.save(config.clone()).await.unwrap();
    assert_eq!(saved.revision, 1);
    assert!(service.save(config).await.is_err());
    service.start().await.unwrap();
    assert!(service.snapshot().unwrap().stats.running);
    let mut invalid = saved.clone();
    invalid.proxy_groups.clear();
    assert!(service.save(invalid).await.is_err());
    assert_eq!(service.config(), &saved);
    service.stop().await.unwrap();
    assert!(!service.snapshot().unwrap().stats.running);
    service.start().await.unwrap();
    service.stop().await.unwrap();
    assert_eq!(Service::open(data.path(), driver).unwrap().config(), &saved);
}

#[tokio::test]
async fn a_disk_failure_does_not_change_the_active_configuration() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path(), data.path().into()).unwrap();
    std::fs::create_dir(data.path().join("config.json")).unwrap();
    let old = service.config().clone();
    let mut next = old.clone();
    next.ui.theme = "dark".into();
    assert!(service.save(next).await.is_err());
    assert_eq!(service.config(), &old);
}

#[tokio::test]
async fn saving_process_dns_does_not_enter_system_dns_recovery() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path(), data.path().into()).unwrap();
    let journal = data.path().join("dns-recovery.json");
    // Any recovery attempt would fail to parse this owned fixture before OS writes.
    std::fs::write(&journal, "invalid test journal").unwrap();
    let mut config = service.config().clone();
    assert!(!config.dns.enabled);
    config.proxy_process_dns = false;
    let saved = service.save(config).await.unwrap();
    assert!(!saved.proxy_process_dns);
    assert!(!saved.dns.enabled);
    assert_eq!(
        std::fs::read_to_string(&journal).unwrap(),
        "invalid test journal"
    );
    std::fs::remove_file(journal).unwrap();
    assert!(
        !Service::open(data.path(), data.path().into())
            .unwrap()
            .config()
            .proxy_process_dns
    );
}

#[cfg(feature = "platform-tests")]
#[tokio::test(flavor = "multi_thread")]
async fn group_replacement_commits_manual_routes_only_after_the_file_is_saved() {
    let _serial = ENGINE_TEST.lock().await;
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(
        data.path(),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/windivert"),
    )
    .unwrap();
    let mut config = service.config().clone();
    let mut group = config.proxy_groups[0].clone();
    group.id = 1;
    group.name = "备用".into();
    config.proxy_groups.push(group);
    service.save(config).await.unwrap();
    service.start().await.unwrap();
    let identity = proxy_platform::process::query(std::process::id())
        .unwrap()
        .identity;
    service.set_process_proxy(&identity, Some(0)).unwrap();
    let path = data.path().join("config.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(service.delete_group(0, 1, 1).await.is_err());
    assert_eq!(
        service
            .snapshot()
            .unwrap()
            .processes
            .iter()
            .find(|p| p.process.identity == identity)
            .unwrap()
            .route
            .as_ref()
            .unwrap()
            .group_id,
        0
    );
    std::fs::remove_dir(&path).unwrap();
    service.delete_group(0, 1, 1).await.unwrap();
    assert_eq!(
        service
            .snapshot()
            .unwrap()
            .processes
            .iter()
            .find(|p| p.process.identity == identity)
            .unwrap()
            .route
            .as_ref()
            .unwrap()
            .group_id,
        1
    );
    service.stop().await.unwrap();
}
