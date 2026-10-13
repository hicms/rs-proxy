use proxy_platform::{connections, process};
use std::{
    net::{TcpListener, UdpSocket},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
};

#[test]
fn process_identity_and_parent_are_read_from_live_windows_processes() {
    let own = process::query(std::process::id()).unwrap();
    assert!(!own.identity.sequence.is_empty());
    assert!(own.name.ends_with(".exe"));
    assert!(own.cmdline.contains("windows_process"));
    assert!(
        process::snapshot()
            .unwrap()
            .iter()
            .any(|p| p.identity == own.identity)
    );
    let old_event = proxy_platform::windivert::timestamp();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut child = Command::new("ping.exe")
        .args(["-n", "30", "127.0.0.1"])
        .creation_flags(0x08000000)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let stale = process::query_socket_ancestors(child.id(), old_event);
    let current =
        process::query_socket_ancestors(child.id(), proxy_platform::windivert::timestamp());
    let actual = process::query(child.id());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(stale.is_err());
    assert!(current.is_ok());
    let actual = actual.unwrap();
    assert_eq!(actual.parent, Some(own.identity));
}

#[test]
fn network_table_reports_real_tcp_and_udp_owners() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let rows = connections::snapshot().unwrap();
    assert!(rows.iter().any(|c| c.pid == std::process::id()
        && c.local_port == listener.local_addr().unwrap().port()
        && c.state == "LISTEN"));
    assert!(rows.iter().any(|c| c.pid == std::process::id()
        && c.local_port == udp.local_addr().unwrap().port()
        && c.state == "BOUND"));
}
