use proxy_core::config::ProxyGroup;
use proxy_engine::dns::DnsForwarder;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Duration, timeout},
};

fn query() -> Vec<u8> {
    let mut bytes = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    bytes.extend(b"\x03one\x04test\0\0\x01\0\x01");
    bytes
}

async fn read_query(socket: &mut TcpStream) {
    let mut hello = [0; 3];
    socket.read_exact(&mut hello).await.unwrap();
    assert_eq!(hello, [5, 1, 0]);
    socket.write_all(&[5, 0]).await.unwrap();
    let mut request = [0; 10];
    socket.read_exact(&mut request).await.unwrap();
    assert_eq!(&request[..4], &[5, 1, 0, 1]);
    socket
        .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
        .await
        .unwrap();
    let size = socket.read_u16().await.unwrap();
    let mut bytes = vec![0; size as usize];
    socket.read_exact(&mut bytes).await.unwrap();
    assert_eq!(bytes, query());
}

#[tokio::test]
async fn dns_forwarding_rejects_malformed_and_mismatched_answers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        for case in 0..5 {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_query(&mut socket).await;
            let mut answer = query();
            answer[2] |= 0x80;
            match case {
                0 => {
                    answer.pop();
                }
                1 => answer[0] ^= 1,
                2 => answer[2] &= 0x7f,
                3 => answer[2] |= 0x08,
                _ => answer[13] ^= 1,
            }
            socket.write_u16(answer.len() as u16).await.unwrap();
            socket.write_all(&answer).await.unwrap();
        }
    });
    let mut forwarder = DnsForwarder::start(
        "127.0.0.1:0".parse().unwrap(),
        "1.1.1.1:53".parse().unwrap(),
        ProxyGroup {
            port,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for _ in 0..5 {
        let mut socket = TcpStream::connect(forwarder.address()).await.unwrap();
        let query = query();
        socket.write_u16(query.len() as u16).await.unwrap();
        socket.write_all(&query).await.unwrap();
        let mut received = Vec::new();
        timeout(Duration::from_secs(3), socket.read_to_end(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert!(
            received.is_empty(),
            "invalid answers must not reach clients"
        );
    }
    server.await.unwrap();
    forwarder.shutdown().await;
}

#[tokio::test]
async fn oversized_udp_answer_is_truncated_and_tcp_retry_keeps_the_full_answer() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut expected = query();
    expected[2] |= 0x80;
    expected[7] = 40;
    for _ in 0..40 {
        expected.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 198, 19, 99, 9]);
    }
    let answer = expected.clone();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_query(&mut socket).await;
            socket.write_u16(answer.len() as u16).await.unwrap();
            socket.write_all(&answer).await.unwrap();
        }
    });
    let mut forwarder = DnsForwarder::start(
        "127.0.0.1:0".parse().unwrap(),
        "1.1.1.1:53".parse().unwrap(),
        ProxyGroup {
            port,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let query = query();
    socket.send_to(&query, forwarder.address()).await.unwrap();
    let mut bytes = vec![0; 1024];
    let (size, _) = timeout(Duration::from_secs(3), socket.recv_from(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(size <= 512);
    assert_eq!(bytes[2] & 0x82, 0x82);
    assert_eq!(&bytes[6..8], &[0, 0]);
    assert_eq!(&bytes[12..size], &query[12..]);
    let mut tcp = TcpStream::connect(forwarder.address()).await.unwrap();
    tcp.write_u16(query.len() as u16).await.unwrap();
    tcp.write_all(&query).await.unwrap();
    let size = timeout(Duration::from_secs(3), tcp.read_u16())
        .await
        .unwrap()
        .unwrap();
    let mut received = vec![0; size as usize];
    tcp.read_exact(&mut received).await.unwrap();
    assert_eq!(received, expected);
    server.await.unwrap();
    forwarder.shutdown().await;
}
