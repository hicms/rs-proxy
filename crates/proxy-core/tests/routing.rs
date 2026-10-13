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
