use proxy_core::{
    config::{AutoRule, Config, Protocol, ProxyGroup},
    model::{ProcessIdentity, ProcessInfo},
    rules::{RoutingPolicy, cmdline_match, wildcard_match},
    tree::ProcessTree,
};

fn process(pid: u32, sequence: u64, name: &str, parent: Option<ProcessIdentity>) -> ProcessInfo {
    ProcessInfo {
        identity: ProcessIdentity {
            pid,
            sequence: sequence.to_string(),
        },
        parent,
        name: name.into(),
        image_path: format!("C:\\Apps\\{name}"),
        cmdline: "app --profile work --renderer".into(),
    }
}
fn policy() -> RoutingPolicy {
    let mut config = Config::default();
    config.proxy_groups.push(ProxyGroup {
        id: 2,
        ..Default::default()
    });
    config.auto_rules.push(AutoRule {
        id: "first".into(),
        name: "主进程".into(),
        enabled: true,
        process_name: "BROWSER.*".into(),
        protocol: Protocol::Tcp,
        ..Default::default()
    });
    config.auto_rules.push(AutoRule {
        id: "second".into(),
        name: "全部".into(),
        enabled: true,
        process_name: "*".into(),
        proxy_group_id: 2,
        ..Default::default()
    });
    RoutingPolicy::new(&config).unwrap()
}

#[test]
fn matching_is_case_insensitive_and_command_keywords_are_conjunctive() {
    assert!(wildcard_match("B?ow*exe", "browser.exe"));
    assert!(!wildcard_match("*.exe", "browser.exe.bak"));
    assert!(wildcard_match("*中文?", "前缀中文名"));
    assert!(cmdline_match("WORK --profile", "app --profile work"));
    assert!(!cmdline_match("work private", "app --profile work"));
    assert!(cmdline_match("*--profile ?ork*", "app --profile work"));
}

#[test]
fn out_of_order_starts_do_not_resurrect_an_exited_or_replaced_identity() {
    let mut tree = ProcessTree::default();
    let old = process(42, 100, "browser.exe", None);
    tree.exit(&old.identity);
    tree.upsert(old.clone());
    assert!(tree.identity(42).is_none());
    let new = process(42, 200, "worker.exe", None);
    tree.upsert(new.clone());
    tree.upsert(old);
    assert_eq!(tree.identity(42), Some(&new.identity));
}

#[test]
fn rule_order_and_protocol_apply_to_descendants_after_parent_exit() {
    let policy = policy();
    let mut tree = ProcessTree::default();
    let parent = process(10, 100, "browser.exe", None);
    let child = process(11, 101, "worker.exe", Some(parent.identity.clone()));
    tree.upsert(child.clone());
    tree.upsert(parent.clone());
    assert_eq!(
        tree.route(&child.identity, Protocol::Tcp, &policy)
            .unwrap()
            .group_id,
        0
    );
    assert_eq!(
        tree.route(&child.identity, Protocol::Udp, &policy)
            .unwrap()
            .group_id,
        2
    );
    tree.exit(&parent.identity);
    assert_eq!(
        tree.route(&child.identity, Protocol::Tcp, &policy)
            .unwrap()
            .rule_id
            .as_deref(),
        Some("first")
    );
    let reused = process(10, 200, "different.exe", None);
    tree.upsert(reused.clone());
    tree.exit(&parent.identity);
    assert_eq!(tree.identity(10), Some(&reused.identity));
    assert_eq!(
        tree.route(&reused.identity, Protocol::Tcp, &policy)
            .unwrap()
            .group_id,
        2
    );
    assert_eq!(
        tree.route(&child.identity, Protocol::Tcp, &policy)
            .unwrap()
            .group_id,
        0
    );
}

#[test]
fn manual_tree_routing_is_identity_scoped_and_precedes_rules() {
    let policy = policy();
    let mut tree = ProcessTree::default();
    let parent = process(10, 100, "browser.exe", None);
    tree.upsert(parent.clone());
    assert!(tree.set_manual(10, Some(2)));
    let child = process(11, 101, "worker.exe", Some(parent.identity.clone()));
    tree.upsert(child.clone());
    let route = tree.route(&child.identity, Protocol::Tcp, &policy).unwrap();
    assert_eq!((route.group_id, route.source.as_str()), (2, "manual"));
    tree.reassign_group(2, 0);
    assert!(!tree.uses_group(2));
    assert_eq!(
        tree.route(&child.identity, Protocol::Tcp, &policy)
            .unwrap()
            .group_id,
        0
    );
    tree.set_manual(10, None);
    assert_eq!(
        tree.route(&child.identity, Protocol::Tcp, &policy)
            .unwrap()
            .source,
        "rule"
    );
    tree.set_manual(10, Some(2));
    tree.exit(&child.identity);
    tree.exit(&parent.identity);
    let reused = process(10, 300, "browser.exe", None);
    tree.upsert(reused.clone());
    assert_eq!(
        tree.route(&reused.identity, Protocol::Tcp, &policy)
            .unwrap()
            .source,
        "rule"
    );
}

#[test]
fn process_dns_overrides_only_global_direct_ranges_and_uses_original_rule_filters() {
    use proxy_core::{config::TrafficFilter, model::Route};
    let mut config = Config::default();
    config.dns.upstream_host = "1.1.1.1".into();
    config.dns.upstream_port = 5353;
    let manual = Route {
        group_id: 0,
        rule_id: None,
        source: "manual".into(),
    };
    let lan = "192.168.1.1".parse().unwrap();
    let public = "198.19.99.53".parse().unwrap();
    let policy = RoutingPolicy::new(&config).unwrap();
    assert_eq!(
        policy
            .destination(&manual, lan, 53)
            .unwrap()
            .address
            .to_string(),
        "1.1.1.1:5353"
    );
    assert!(policy.destination(&manual, lan, 443).is_none());
    assert!(
        policy
            .destination(&manual, "127.0.0.1".parse().unwrap(), 53)
            .is_none()
    );
    assert!(!policy.destination(&manual, public, 443).unwrap().dns);
    config.auto_rules.push(AutoRule {
        id: "dns".into(),
        name: "DNS".into(),
        enabled: true,
        process_name: "browser.exe".into(),
        protocol: Protocol::Udp,
        dst_filter: TrafficFilter {
            include_cidrs: vec!["192.168.0.0/16".into()],
            exclude_cidrs: vec!["192.168.1.2".into()],
            include_ports: vec!["53".into()],
            ..Default::default()
        },
        ..Default::default()
    });
    let policy = RoutingPolicy::new(&config).unwrap();
    let browser = process(42, 1, "browser.exe", None);
    assert!(policy.classify(&[&browser], Protocol::Tcp).is_none());
    let rule = policy.classify(&[&browser], Protocol::Udp).unwrap();
    assert!(policy.destination(&rule, lan, 53).is_some());
    assert!(policy.destination(&rule, public, 53).is_none());
    assert!(
        policy
            .destination(&rule, "192.168.1.2".parse().unwrap(), 53)
            .is_none()
    );
    assert!(policy.destination(&rule, lan, 443).is_none());
    config.proxy_process_dns = false;
    let policy = RoutingPolicy::new(&config).unwrap();
    assert!(policy.destination(&manual, lan, 53).is_none());
    let direct_target = policy.destination(&manual, public, 53).unwrap();
    assert!(!direct_target.dns);
    assert_eq!(direct_target.address.to_string(), "198.19.99.53:53");
}
