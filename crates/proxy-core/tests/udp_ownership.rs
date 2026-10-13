use proxy_core::{
    flow::FlowKey,
    model::ProcessIdentity,
    udp::{UdpLookup, UdpOwner, UdpTable},
};
fn owner(endpoint: u64, ip: &str, timestamp: i64) -> UdpOwner {
    UdpOwner {
        endpoint,
        identity: ProcessIdentity {
            pid: 10,
            sequence: "100".into(),
        },
        timestamp,
        local: format!("{ip}:4000").parse().unwrap(),
    }
}
fn key(ip: &str) -> FlowKey {
    FlowKey {
        local: format!("{ip}:4000").parse().unwrap(),
        remote: "198.19.99.1:53".parse().unwrap(),
    }
}
#[test]
fn closing_one_endpoint_preserves_another_socket_on_the_same_port() {
    let mut table = UdpTable::default();
    let first = owner(1, "10.0.0.1", 100);
    let second = owner(2, "10.0.0.2", 110);
    table.bind(first.clone(), false, None);
    table.bind(second.clone(), false, None);
    table.close(1, 10, first.local, 120);
    assert!(!table.active(1, &first.identity));
    assert!(table.active(2, &second.identity));
    assert!(matches!(table.lookup(&key("10.0.0.2"), 130), UdpLookup::Found(o) if o.endpoint == 2));
}
#[test]
fn a_parked_packet_cannot_be_claimed_by_a_reused_socket() {
    let mut table = UdpTable::default();
    let first = owner(1, "0.0.0.0", 100);
    table.bind(first.clone(), false, None);
    table.close(1, 10, first.local, 120);
    table.bind(owner(2, "0.0.0.0", 130), false, None);
    assert!(matches!(
        table.lookup(&key("10.0.0.1"), 110),
        UdpLookup::Stale
    ));
    assert!(matches!(table.lookup(&key("10.0.0.1"), 140), UdpLookup::Found(o) if o.endpoint == 2));
}
#[test]
fn ambiguous_live_sockets_are_not_assigned_to_an_arbitrary_session() {
    let mut table = UdpTable::default();
    table.bind(owner(1, "0.0.0.0", 100), false, None);
    table.bind(owner(2, "0.0.0.0", 110), false, None);
    assert!(matches!(
        table.lookup(&key("10.0.0.1"), 120),
        UdpLookup::Stale
    ));
}

#[test]
fn a_seeded_wildcard_is_replaced_by_its_concrete_endpoint_and_does_not_poison_reuse() {
    let mut table = UdpTable::default();
    table.bind(owner(100, "0.0.0.0", 0), true, None);
    let actual = owner(1, "10.0.0.1", 100);
    table.bind(actual.clone(), false, Some(key("10.0.0.1").remote));
    assert!(!table.active(100, &actual.identity));
    table.close(1, 10, actual.local, 120);
    table.bind(owner(2, "0.0.0.0", 130), false, None);
    assert!(matches!(table.lookup(&key("10.0.0.1"), 140), UdpLookup::Found(o) if o.endpoint == 2));
}
