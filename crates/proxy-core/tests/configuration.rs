use proxy_core::{
    config::{AutoRule, Config, TrafficFilter},
    filter::CompiledFilter,
    store::{ConfigStore, StoreError},
};
use std::{fs, net::Ipv4Addr};

#[test]
fn defaults_leave_traffic_untouched_and_roundtrip() {
    let config = Config::default();
    config.validate().unwrap();
    assert!(!config.dns.enabled);
    assert!(config.proxy_process_dns);
    let mut saved = serde_json::to_value(&config).unwrap();
    saved.as_object_mut().unwrap().remove("proxy_process_dns");
    assert!(
        serde_json::from_value::<Config>(saved)
            .unwrap()
            .proxy_process_dns
    );
    assert!(config.auto_rules.is_empty());
    assert!(!AutoRule::default().enabled);
    assert_eq!(
        serde_json::from_str::<Config>(&serde_json::to_string(&config).unwrap()).unwrap(),
        config
    );
}

#[test]
fn invalid_references_and_duplicate_groups_are_rejected() {
    let mut config = Config::default();
    config.auto_rules.push(AutoRule {
        id: "browser".into(),
        name: "浏览器".into(),
        process_name: "browser.exe".into(),
        proxy_group_id: 99,
        ..Default::default()
    });
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("代理组不存在")
    );
    config.auto_rules[0].proxy_group_id = 0;
    config.validate().unwrap();
    config.proxy_groups.push(config.proxy_groups[0].clone());
    assert!(config.validate().unwrap_err().to_string().contains("重复"));
}

#[test]
fn excludes_win_and_includes_restrict_address_and_port() {
    let filter = CompiledFilter::new(&TrafficFilter {
        include_cidrs: vec!["198.18.0.0/24".into()],
        exclude_cidrs: vec!["198.18.0.9".into()],
        include_ports: vec!["8000-8100".into()],
        exclude_ports: vec!["8080".into()],
    })
    .unwrap();
    assert!(filter.allows(Ipv4Addr::new(198, 18, 0, 8), 8000));
    assert!(!filter.allows(Ipv4Addr::new(198, 18, 0, 9), 8000));
    assert!(!filter.allows(Ipv4Addr::new(198, 18, 0, 8), 8080));
    assert!(!filter.allows(Ipv4Addr::new(198, 18, 1, 8), 8000));
    assert!(!filter.allows(Ipv4Addr::new(198, 18, 0, 8), 443));
    assert!(
        CompiledFilter::new(&TrafficFilter {
            include_ports: vec!["100-1".into()],
            ..Default::default()
        })
        .is_err()
    );
    assert!(
        CompiledFilter::new(&TrafficFilter {
            include_cidrs: vec!["1.2.3.4/99".into()],
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn store_rejects_stale_writes_and_retains_current_on_io_failure() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    let mut store = ConfigStore::open(&path).unwrap();
    let stale = store.config().clone();
    let mut changed = stale.clone();
    changed.ui.theme = "dark".into();
    let saved = store.replace(changed).unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(ConfigStore::open(&path).unwrap().config(), &saved);
    assert!(matches!(store.replace(stale), Err(StoreError::Conflict)));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let mut next = saved.clone();
    next.ui.theme = "light".into();
    assert!(matches!(store.replace(next), Err(StoreError::Io(_))));
    assert_eq!(store.config(), &saved);
}

#[test]
fn corrupt_existing_configuration_is_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    fs::write(&path, "{broken").unwrap();
    assert!(matches!(ConfigStore::open(&path), Err(StoreError::Json(_))));
    assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
}
