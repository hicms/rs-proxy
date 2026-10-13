use crate::{
    packet, process_dns,
    socks::{self, Association, SocksAddress},
    state::{Shared, UdpRoute},
    traffic::Direction,
};
use anyhow::{Context, Result};
use proxy_platform::windivert::{Address, DivertHandle};
use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::mpsc, task::JoinSet, time::interval};
use tokio_util::sync::CancellationToken;

pub(crate) async fn tcp_loop(
    listener: TcpListener,
    shared: Arc<Shared>,
    cancel: CancellationToken,
) {
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let (mut client,peer) = match accepted {
                    Ok(value) => value,
                    Err(error) if matches!(error.kind(), std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock | std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset) => { continue; },
                    Err(error) => { shared.error(error); shared.stopping.store(true, Ordering::Release); break; }
                };
                let entry = client.local_addr().ok().and_then(|local| match (local,peer) {
                    (SocketAddr::V4(local),SocketAddr::V4(peer)) => {
                        let mut state = shared.state.lock().unwrap();
                        let entry = state.reverse.get(&(*local.ip(),*peer.ip(),peer.port())).cloned();
                        if let Some(entry) = &entry { state.accepted(entry); }
                        entry
                    }
                    _ => None,
                });
                let Some(entry) = entry else { continue; };
                let shared = shared.clone(); let cancel = cancel.clone();
                tasks.spawn(async move {
                    let meter = shared.traffic.register(&entry.identity, &entry.traffic_group);
                    shared.counters.tcp.fetch_add(1,Ordering::Relaxed);
                    let result: Result<()> = tokio::select! {
                        _ = cancel.cancelled() => Ok(()),
                        result = async {
                            let mut upstream = socks::connect(&entry.group,SocketAddr::V4(entry.destination.address)).await
                                .with_context(|| format!("建立 SOCKS5 连接（代理组 {}，目标 {}，上游 {}）", entry.group.name, entry.key.remote, entry.destination.address))?;
                            let (client_read, client_write) = client.split(); let (proxy_read, proxy_write) = upstream.split();
                            match tokio::try_join!(meter.transfer(client_read,proxy_write,Direction::Upload),meter.transfer(proxy_read,client_write,Direction::Download)) {
                                Ok(_) => {},
                                // A reset ends this established stream; it does not stop the proxy engine.
                                Err(error) if matches!(error.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe | ErrorKind::NotConnected) => {
                                    tracing::debug!(pid = entry.identity.pid, group_id = entry.group.id, destination = %entry.key.remote, %error, "TCP 会话已关闭");
                                },
                                Err(error) => return Err(error.into()),
                            }
                            Ok(())
                        } => result,
                    };
                    if let Err(error) = result { shared.error(format!("TCP 转发失败：{error:#}")); }
                    shared.counters.tcp.fetch_sub(1,Ordering::Relaxed);
                    drop(client);
                    shared.state.lock().unwrap().finish(&entry);
                });
            }
        }
    }
    cancel.cancel();
    while tasks.join_next().await.is_some() {}
}

pub(crate) struct UdpPacket {
    pub route: UdpRoute,
    pub local: SocketAddrV4,
    pub remote: SocketAddrV4,
    pub payload: Vec<u8>,
    pub address: Address,
}
type SessionKey = (u64, i64, SocketAddrV4, u64);

async fn udp_session(
    first: UdpPacket,
    mut receiver: mpsc::Receiver<UdpPacket>,
    shared: Arc<Shared>,
    network: Arc<DivertHandle>,
    cancel: CancellationToken,
) -> Result<()> {
    let Association {
        mut control,
        socket,
    } = socks::Association::open(&first.route.group).await?;
    let owner = first.route.owner.clone();
    let meter = shared
        .traffic
        .register(&owner.identity, &first.route.traffic_group);
    let local = first.local;
    let mut address = first.address;
    let first_payload = socks::encode_datagram(SocketAddr::V4(first.remote), &first.payload)?;
    socket.send(&first_payload).await?;
    meter.record(first.payload.len(), 0);
    let mut buffer = vec![0; 65535];
    let mut control_byte = [0; 1];
    let mut expiry = interval(Duration::from_secs(1));
    let mut last_activity = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = control.read(&mut control_byte) => return Ok(()),
            _ = expiry.tick() => {
                if last_activity.elapsed() > Duration::from_secs(120) || !shared.state.lock().unwrap().udp_active(&owner) { return Ok(()); }
            }
            packet = receiver.recv() => {
                let Some(packet) = packet else { return Ok(()); }; address = packet.address;
                socket.send(&socks::encode_datagram(SocketAddr::V4(packet.remote),&packet.payload)?).await?;
                meter.record(packet.payload.len(), 0); last_activity = tokio::time::Instant::now();
            }
            received = socket.recv(&mut buffer) => {
                let count = received?; let (remote,payload) = socks::decode_datagram(&buffer[..count])?;
                let SocksAddress::Ip(SocketAddr::V4(remote)) = remote else { continue; };
                if !shared.state.lock().unwrap().udp_active(&owner) { return Ok(()); }
                let mut packet = packet::udp_reply(remote,local,payload).context("UDP 回复超过 IPv4 长度限制")?;
                let mut reply_address = address; reply_address.set_inbound();
                network.recalculate(&mut packet,&mut reply_address)?; network.send(&packet,&reply_address)?;
                meter.record(0, payload.len()); last_activity = tokio::time::Instant::now();
            }
        }
    }
}

pub(crate) async fn udp_loop(
    mut receiver: mpsc::Receiver<UdpPacket>,
    shared: Arc<Shared>,
    network: Arc<DivertHandle>,
    cancel: CancellationToken,
) {
    let mut sessions: HashMap<SessionKey, mpsc::Sender<UdpPacket>> = HashMap::new();
    let mut tasks = JoinSet::new();
    let mut queries = JoinSet::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            Some(result) = queries.join_next(), if !queries.is_empty() => {
                if let Ok(Err(error)) = result { shared.error(format!("DNS 转发失败：{error:#}")); }
            }
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if let Ok((key,result)) = result { sessions.remove(&key); if let Err(error) = result { shared.error(format!("UDP 转发失败：{error:#}")); } }
            }
            packet = receiver.recv() => {
                let Some(packet) = packet else { break; };
                if packet.route.destination.dns {
                    if queries.len() >= 256 { shared.error("DNS 查询达到容量限制，查询已丢弃"); continue; }
                    let shared = shared.clone(); let network = network.clone(); let cancel = cancel.clone();
                    queries.spawn(async move {
                        shared.counters.udp.fetch_add(1,Ordering::Relaxed);
                        let result = tokio::select! { _ = cancel.cancelled() => Ok(()), result = process_dns::relay(packet,&shared,&network) => result };
                        shared.counters.udp.fetch_sub(1,Ordering::Relaxed);
                        result
                    });
                    continue;
                }
                let key = (packet.route.owner.endpoint,packet.route.owner.timestamp,packet.local,packet.route.traffic_group.generation());
                if let Some(sender) = sessions.get(&key) {
                    if sender.try_send(packet).is_err() { shared.error("UDP 会话队列不可用，数据报已丢弃"); } continue;
                }
                if sessions.len() >= 2048 { shared.error("UDP 会话达到容量限制，数据报已丢弃"); continue; }
                let (sender,receiver) = mpsc::channel(256); sessions.insert(key,sender);
                let shared = shared.clone(); let network = network.clone(); let cancel = cancel.clone();
                tasks.spawn(async move {
                    shared.counters.udp.fetch_add(1,Ordering::Relaxed);
                    let result = tokio::select! { _ = cancel.cancelled() => Ok(()), result = udp_session(packet,receiver,shared.clone(),network,cancel.clone()) => result };
                    shared.counters.udp.fetch_sub(1,Ordering::Relaxed); (key,result)
                });
            }
        }
    }
    cancel.cancel();
    while tasks.join_next().await.is_some() {}
    while queries.join_next().await.is_some() {}
}
