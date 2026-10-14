use proxy_core::config::ProxyGroup;
use proxy_engine::dns::DnsForwarder;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Duration, sleep, timeout},
};

#[tokio::test]
async fn udp_and_tcp_dns_queries_use_the_configured_socks_server() {
    let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = server.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = server.accept().await.unwrap();
            let mut hello = [0; 3];
            stream.read_exact(&mut hello).await.unwrap();
            assert_eq!(hello, [5, 1, 0]);
            stream.write_all(&[5, 0]).await.unwrap();
            let mut connect = [0; 10];
            stream.read_exact(&mut connect).await.unwrap();
            assert_eq!(connect, [5, 1, 0, 1, 8, 8, 8, 8, 0, 53]);
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                .await
                .unwrap();
            let length = stream.read_u16().await.unwrap();
            let mut query = vec![0; length as usize];
            stream.read_exact(&mut query).await.unwrap();
            query[2] |= 0x80;
            stream.write_u16(length).await.unwrap();
            stream.write_all(&query).await.unwrap();
        }
    });
    let mut forwarder = DnsForwarder::start(
        "127.0.0.1:0".parse().unwrap(),
        "8.8.8.8:53".parse().unwrap(),
        ProxyGroup {
            port,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let fixed = forwarder.address();
    assert!(
        DnsForwarder::start(
            fixed,
            "8.8.8.8:53".parse().unwrap(),
            ProxyGroup {
                port,
                ..Default::default()
            }
        )
        .await
        .is_err(),
        "a fixed occupied port must not fall back to another port"
    );
    let query = [
        0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 3, b'o', b'n', b'e', 4, b't', b'e', b's', b't',
        0, 0, 1, 0, 1,
    ];
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp.send_to(&query, forwarder.address()).await.unwrap();
    let mut response = [0; 512];
    let (count, from) = timeout(Duration::from_secs(3), udp.recv_from(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count, query.len());
    assert_eq!(from, forwarder.address().into());
    assert_eq!(&response[..3], &[0x12, 0x34, 0x81]);
    let mut tcp = TcpStream::connect(forwarder.address()).await.unwrap();
    tcp.write_u16(query.len() as u16).await.unwrap();
    tcp.write_all(&query).await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(3), tcp.read_u16())
            .await
            .unwrap()
            .unwrap(),
        query.len() as u16
    );
    tcp.read_exact(&mut response[..query.len()]).await.unwrap();
    assert_eq!(&response[..3], &[0x12, 0x34, 0x81]);
    task.await.unwrap();
    forwarder.shutdown().await;

    // Failure to bind TCP must release the UDP socket reserved for the pair.
    let tcp_only = TcpListener::bind(fixed).await.unwrap();
    assert!(
        DnsForwarder::start(
            fixed,
            "8.8.8.8:53".parse().unwrap(),
            ProxyGroup {
                port,
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
    let udp_only = UdpSocket::bind(fixed).await.unwrap();
    assert_eq!(
        tcp_only.local_addr().unwrap(),
        udp_only.local_addr().unwrap()
    );
}

#[tokio::test]
async fn a_reply_to_a_closed_client_port_must_not_stop_the_forwarder() {
    let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = server.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let mut first = true;
        for _ in 0..8 {
            let Ok((mut stream, _)) = server.accept().await else {
                break;
            };
            let delayed = first;
            first = false;
            let mut hello = [0; 3];
            stream.read_exact(&mut hello).await.unwrap();
            assert_eq!(hello, [5, 1, 0]);
            stream.write_all(&[5, 0]).await.unwrap();
            let mut connect = [0; 10];
            stream.read_exact(&mut connect).await.unwrap();
            assert_eq!(connect, [5, 1, 0, 1, 8, 8, 8, 8, 0, 53]);
            stream
                .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
                .await
                .unwrap();
            let length = stream.read_u16().await.unwrap();
            let mut query = vec![0; length as usize];
            stream.read_exact(&mut query).await.unwrap();
            if delayed {
                // The first client closes its socket before this reply is written.
                sleep(Duration::from_millis(150)).await;
            }
            query[2] |= 0x80;
            stream.write_u16(length).await.unwrap();
            stream.write_all(&query).await.unwrap();
        }
    });
    let mut forwarder = DnsForwarder::start(
        "127.0.0.1:0".parse().unwrap(),
        "8.8.8.8:53".parse().unwrap(),
        ProxyGroup {
            port,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let query = [
        0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 3, b'o', b'n', b'e', 4, b't', b'e', b's', b't',
        0, 0, 1, 0, 1,
    ];
    // The client abandons the query; the forwarder's reply reaches a closed
    // port and Windows surfaces the resulting reset on the next receive.
    {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        udp.send_to(&query, forwarder.address()).await.unwrap();
    }
    sleep(Duration::from_millis(400)).await;
    for _ in 0..5 {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        udp.send_to(&query, forwarder.address()).await.unwrap();
        let mut response = [0; 512];
        if let Ok(Ok((count, from))) =
            timeout(Duration::from_secs(2), udp.recv_from(&mut response)).await
        {
            assert_eq!(count, query.len());
            assert_eq!(from, forwarder.address().into());
            assert_eq!(&response[..3], &[0x12, 0x34, 0x81]);
            forwarder.shutdown().await;
            task.abort();
            return;
        }
    }
    panic!("the forwarder stopped answering after a client closed its port");
}
