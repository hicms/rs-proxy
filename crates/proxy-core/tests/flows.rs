use proxy_core::{
    flow::{FlowKey, FlowTable},
    model::{ProcessIdentity, Route},
};

fn key() -> FlowKey {
    FlowKey {
        local: "10.0.0.2:45000".parse().unwrap(),
        remote: "198.18.0.1:443".parse().unwrap(),
    }
}
fn route() -> Route {
    Route {
        group_id: 0,
        rule_id: None,
        source: "manual".into(),
    }
}

#[test]
fn abandoned_syn_stays_direct_despite_late_decisions_and_retransmits() {
    let mut table = FlowTable::default();
    table.abandon(key(), 1234, 100);
    assert!(!table.publish(key(), 90, 90, Some(route()), None));
    assert_eq!(table.on_syn(&key(), 1234, 10000, 20), Some(None));
    assert_eq!(table.on_syn(&key(), 5678, 20000, 20), None);
    assert!(table.publish(key(), 20001, 20001, Some(route()), None));
    assert_eq!(table.on_syn(&key(), 5678, 20002, 20), Some(Some(route())));
}

#[test]
fn old_close_or_relay_cleanup_cannot_remove_a_new_flow() {
    let mut table = FlowTable::default();
    table.publish(key(), 100, 100, Some(route()), None);
    assert_eq!(table.on_syn(&key(), 10, 101, 20), Some(Some(route())));
    assert!(table.publish(key(), 102, 102, None, None));
    assert_eq!(table.on_syn(&key(), 20, 200, 20), None);
    table.publish(key(), 200, 200, Some(route()), None);
    table.release(&key(), 100);
    table.close(&key(), 100);
    assert_eq!(table.get(&key()).unwrap().timestamp, 200);
    table.release(&key(), 200);
    assert!(table.get(&key()).is_none());
}

#[test]
fn stale_decisions_do_not_classify_reused_ports() {
    let mut table = FlowTable::default();
    table.publish(key(), 100, 100, Some(route()), None);
    assert!(!table.publish(key(), 99, 99, None, None));
    assert_eq!(table.on_syn(&key(), 10, 500, 20), None);
    table.publish(key(), 501, 501, None, None);
    table.close(&key(), 499);
    assert!(table.get(&key()).is_some());
    table.close(&key(), 501);
    assert!(table.get(&key()).is_none());
}

#[test]
fn a_late_connect_supplies_cleanup_identity_without_changing_a_direct_flow() {
    let mut table = FlowTable::default();
    table.abandon(key(), 10, 100);
    assert!(!table.publish(key(), 7, 90, Some(route()), None));
    assert_eq!(table.on_syn(&key(), 10, 101, 20), Some(None));
    table.close(&key(), 7);
    assert!(table.get(&key()).is_none());
}

#[test]
fn connect_before_reused_syn_preserves_both_generations() {
    let mut table = FlowTable::default();
    table.publish(
        key(),
        1,
        100,
        Some(route()),
        Some(ProcessIdentity {
            pid: 42,
            sequence: "1".into(),
        }),
    );
    assert_eq!(table.on_syn(&key(), 10, 101, 20), Some(Some(route())));
    assert!(table.publish(
        key(),
        2,
        200,
        Some(route()),
        Some(ProcessIdentity {
            pid: 42,
            sequence: "2".into()
        })
    ));
    assert_eq!(table.on_syn(&key(), 10, 201, 20), Some(Some(route())));
    assert_eq!(
        table
            .get(&key())
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .sequence,
        "1"
    );
    table.close(&key(), 1);
    assert_eq!(table.on_syn(&key(), 20, 202, 20), Some(Some(route())));
    table.release(&key(), 100);
    assert_eq!(table.get(&key()).unwrap().endpoint, Some(2));
    assert_eq!(
        table
            .get(&key())
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .sequence,
        "2"
    );
}
