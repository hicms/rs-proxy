use proxy_platform::{connections, process};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
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
    let listener6 = TcpListener::bind("[::1]:0").unwrap();
    assert!(
        connections::tcp6_listener_owners()
            .unwrap()
            .contains(&(std::process::id(), listener6.local_addr().unwrap().port()))
    );
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let rows = connections::snapshot().unwrap();
    assert!(rows.iter().any(|c| c.pid == std::process::id()
        && c.local_port == listener.local_addr().unwrap().port()
        && c.state == "LISTEN"));
    assert!(rows.iter().any(|c| c.pid == std::process::id()
        && c.local_port == udp.local_addr().unwrap().port()
        && c.state == "BOUND"));

    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    let mut control = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut control_peer, _) = listener.accept().unwrap();
    drop(listener);
    let identity = process::query(std::process::id()).unwrap().identity;
    let tcp = connections::tcp_snapshot().unwrap();
    let outgoing = tcp
        .iter()
        .find(|c| {
            c.pid == identity.pid && c.key.local.port() == client.local_addr().unwrap().port()
        })
        .unwrap();
    let incoming = tcp
        .iter()
        .find(|c| {
            c.pid == identity.pid && c.key.remote.port() == client.local_addr().unwrap().port()
        })
        .unwrap();
    assert!(outgoing.is_established());
    assert!(
        !connections::disconnect_tcp(incoming, &identity).unwrap(),
        "accepted connection must survive even after its listener closes"
    );
    let stale = proxy_core::model::ProcessIdentity {
        sequence: "stale".into(),
        ..identity.clone()
    };
    assert!(!connections::disconnect_tcp(outgoing, &stale).unwrap());
    client.write_all(b"a").unwrap();
    accepted
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut byte = [0];
    accepted.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [b'a']);
    assert!(connections::disconnect_tcp(outgoing, &identity).unwrap());
    assert!(
        !connections::disconnect_tcp(outgoing, &identity).unwrap(),
        "a closed candidate must not be deleted again"
    );
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    match client.read(&mut byte) {
        Ok(count) => assert_eq!(count, 0),
        Err(error) => assert!(!matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )),
    }
    control.write_all(b"b").unwrap();
    control_peer
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    control_peer.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [b'b'], "unrelated connection remains usable");
}
