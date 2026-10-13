use crate::socks;
use anyhow::{Context, Result, bail};
use proxy_core::config::ProxyGroup;
use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket, lookup_host},
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

async fn exchange(group: &ProxyGroup, upstream: SocketAddrV4, query: &[u8]) -> Result<Vec<u8>> {
    if query.len() < 12 || query.len() > u16::MAX as usize || query[2] & 0x80 != 0 {
        bail!("DNS 查询格式无效");
    }
    timeout(Duration::from_secs(8), async {
        let mut stream = socks::connect(group, SocketAddr::V4(upstream)).await?;
        stream.write_u16(query.len() as u16).await?;
        stream.write_all(query).await?;
        let length = stream.read_u16().await?;
        let mut response = vec![0; length as usize];
        stream.read_exact(&mut response).await?;
        if response.len() < 12 || response[..2] != query[..2] || response[2] & 0x80 == 0 {
            bail!("DNS 上游返回了不匹配的响应");
        }
        Ok(response)
    })
    .await
    .context("DNS 转发超时")?
}
async fn tcp_client(
    mut stream: TcpStream,
    group: ProxyGroup,
    upstream: SocketAddrV4,
) -> Result<()> {
    loop {
        let length = match stream.read_u16().await {
            Ok(length) => length,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut query = vec![0; length as usize];
        stream.read_exact(&mut query).await?;
        let response = exchange(&group, upstream, &query).await?;
        stream.write_u16(response.len() as u16).await?;
        stream.write_all(&response).await?;
    }
}

pub struct DnsForwarder {
    address: SocketAddrV4,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl DnsForwarder {
    pub async fn start(
        listen: SocketAddrV4,
        upstream: SocketAddrV4,
        mut group: ProxyGroup,
    ) -> Result<Self> {
        if !listen.ip().is_loopback() {
            bail!("DNS 转发只监听回环地址");
        }
        let resolved = timeout(
            Duration::from_secs(8),
            lookup_host((group.host.as_str(), group.port)),
        )
        .await
        .context("解析 DNS 代理主机超时")??
        .next()
        .context("DNS 代理主机无可用地址")?;
        group.host = resolved.ip().to_string();
        let tcp = TcpListener::bind(listen)
            .await
            .context("监听本地 DNS TCP")?;
        let SocketAddr::V4(address) = tcp.local_addr()? else {
            bail!("DNS 监听地址必须为 IPv4");
        };
        let udp = Arc::new(UdpSocket::bind(address).await.context("监听本地 DNS UDP")?);
        let cancel = CancellationToken::new();
        let stopped = cancel.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            let mut bytes = vec![0; 65535];
            loop {
                tokio::select! {
                    _ = stopped.cancelled() => break,
                    Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                        if let Ok(Err(error)) = result { tracing::warn!(%error,"DNS 查询失败"); }
                    }
                    datagram = udp.recv_from(&mut bytes), if tasks.len() < 256 => {
                        let (length,peer) = match datagram { Ok(value) => value, Err(error) => { tracing::error!(%error,"DNS 接收失败"); break; } };
                        let query = bytes[..length].to_vec(); let group = group.clone(); let udp = udp.clone();
                        tasks.spawn(async move {
                            let response = exchange(&group,upstream,&query).await?;
                            udp.send_to(&response,peer).await?; Ok::<(),anyhow::Error>(())
                        });
                    }
                    client = tcp.accept(), if tasks.len() < 256 => {
                        let (stream,_) = match client { Ok(value) => value, Err(error) => { tracing::error!(%error,"DNS TCP 接收失败"); break; } };
                        let group = group.clone(); tasks.spawn(async move { timeout(Duration::from_secs(30),tcp_client(stream,group,upstream)).await.context("DNS TCP 客户端超时")? });
                    }
                }
            }
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        });
        Ok(Self {
            address,
            cancel,
            task: Some(task),
        })
    }
    pub fn address(&self) -> SocketAddrV4 {
        self.address
    }
    pub async fn shutdown(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for DnsForwarder {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
