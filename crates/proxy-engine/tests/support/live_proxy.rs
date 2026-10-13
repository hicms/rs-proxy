use anyhow::{Context, Result, ensure};
use proxy_core::config::ProxyGroup;
use proxy_engine::{
    dns::DnsForwarder,
    socks::{self, Association, SocksAddress},
};
use std::net::SocketAddr;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    time::{Duration, timeout},
};

fn query(id: u16) -> Vec<u8> {
    let mut bytes = vec![0; 12];
    bytes[..2].copy_from_slice(&id.to_be_bytes());
    bytes[2] = 1;
    bytes[5] = 1;
    bytes.extend(b"\x07example\x03com\0\0\x01\0\x01");
    bytes
}
fn validate(bytes: &[u8], id: u16) -> Result<()> {
    ensure!(bytes.len() >= 12, "truncated DNS response");
    ensure!(bytes[..2] == id.to_be_bytes(), "DNS ID mismatch");
    ensure!(
        bytes[2] & 0x80 != 0 && bytes[3] & 15 == 0,
        "invalid DNS reply or RCODE"
    );
    ensure!(
        u16::from_be_bytes([bytes[6], bytes[7]]) > 0,
        "DNS reply has no answers"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint: SocketAddr = std::env::args()
        .nth(1)
        .context("usage: check-live-proxy 127.0.0.1:7891")?
        .parse()?;
    let group = ProxyGroup {
        host: endpoint.ip().to_string(),
        port: endpoint.port(),
        ..Default::default()
    };
    println!("handshake: {:?}", socks::test(&group).await?);
    timeout(Duration::from_secs(60), async {
        for (index, target) in ["1.1.1.1:53", "8.8.8.8:53"].iter().enumerate() {
            let target: SocketAddr = target.parse()?;
            let id = 4100 + index as u16;
            let payload = query(id);
            let mut tcp = socks::connect(&group, target).await?;
            tcp.write_u16(payload.len() as u16).await?;
            tcp.write_all(&payload).await?;
            let length = tcp.read_u16().await?;
            let mut reply = vec![0; length as usize];
            tcp.read_exact(&mut reply).await?;
            validate(&reply, id)?;
            println!(
                "TCP DNS via {endpoint} to {target}: {} bytes, matching ID and answers",
                reply.len()
            );
        }
        let mut association = Association::open(&group).await?;
        println!("UDP relay: {}", association.socket.peer_addr()?);
        for (index, target) in ["1.1.1.1:53", "1.0.0.1:53"].iter().enumerate() {
            let target: SocketAddr = target.parse()?;
            let id = 4200 + index as u16;
            association
                .socket
                .send(&socks::encode_datagram(target, &query(id))?)
                .await?;
            let mut reply = vec![0; 65535];
            let mut control_byte = [0];
            let count = tokio::select! {
                result = timeout(Duration::from_secs(8), association.socket.recv(&mut reply)) => result.context("UDP response timeout")??,
                result = association.control.read(&mut control_byte) => anyhow::bail!("UDP control stream closed or sent data: {result:?}"),
            };
            let (source, payload) = socks::decode_datagram(&reply[..count])?;
            ensure!(source == SocksAddress::Ip(target), "UDP source mismatch");
            validate(payload, id)?;
            println!(
                "UDP DNS via {endpoint} to {target}: {} bytes, matching source, ID and answers",
                payload.len()
            );
        }
        let mut forwarder =
            DnsForwarder::start("127.0.0.1:0".parse()?, "1.1.1.1:53".parse()?, group).await?;
        let result = async {
            let id = 4300;
            let udp = UdpSocket::bind("127.0.0.1:0").await?;
            udp.send_to(&query(id), forwarder.address()).await?;
            let mut reply = vec![0; 65535];
            let (count, peer) = udp.recv_from(&mut reply).await?;
            ensure!(
                peer == SocketAddr::V4(forwarder.address()),
                "forwarder reply source mismatch"
            );
            validate(&reply[..count], id)?;
            let mut tcp = TcpStream::connect(forwarder.address()).await?;
            let payload = query(id + 1);
            tcp.write_u16(payload.len() as u16).await?;
            tcp.write_all(&payload).await?;
            let count = tcp.read_u16().await? as usize;
            tcp.read_exact(&mut reply[..count]).await?;
            validate(&reply[..count], id + 1)?;
            println!(
                "local UDP/TCP DNS forwarding through {endpoint}: passed; no system DNS changes"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        forwarder.shutdown().await;
        result
    })
    .await
    .context("live proxy test timed out")??;
    Ok(())
}
