use proxy_platform::windivert::{Driver, frequency, timestamp};
use std::{
    net::UdpSocket,
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[test]
fn driver_reports_real_udp_socket_identity_and_shutdown_unblocks_receive() {
    let driver = Driver::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/windivert"),
    )
    .unwrap();
    let handle = driver
        .open(
            &format!("udp and processId == {}", std::process::id()),
            3,
            0,
            1 | 4,
        )
        .unwrap();
    assert_eq!(handle.version().unwrap(), (2, 2));
    let reader = handle.clone();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        while let Ok((_, address)) = reader.receive(&mut []) {
            if let Some(event) = address.socket_event() {
                tx.send(event).unwrap();
            }
        }
    });
    let before = timestamp();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect("127.0.0.1:5353").unwrap();
    socket.send(b"probe").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = None;
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50))
            && event.kind == 4
        {
            found = Some(event);
            break;
        }
    }
    handle.shutdown_receive().unwrap();
    worker.join().unwrap();
    let event = found.expect("未收到 UDP CONNECT 事件");
    assert_eq!(event.pid, std::process::id());
    assert_eq!(event.local.port(), socket.local_addr().unwrap().port());
    assert_eq!(event.remote, "127.0.0.1:5353".parse().unwrap());
    assert!(event.timestamp >= before);
    assert!(frequency() > 0);
}
