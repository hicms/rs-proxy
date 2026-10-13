use crate::{
    socks,
    traffic::{Direction, TrafficMeter},
};
use anyhow::{Context, Result, bail};
use hickory_proto::op::{Message, MessageType, OpCode};
use proxy_core::config::ProxyGroup;
use std::{
    io::ErrorKind,
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

async fn bind_listeners(listen: SocketAddrV4) -> Result<(TcpListener, UdpSocket)> {
    for attempt in 0..32 {
        // Windows can reserve different ephemeral port ranges for TCP and UDP.
        let udp = UdpSocket::bind(listen).await.context("监听本地 DNS UDP")?;
        match TcpListener::bind(udp.local_addr()?).await {
            Ok(tcp) => return Ok((tcp, udp)),
            Err(error)
                if listen.port() == 0
                    && attempt < 31
                    && matches!(
                        error.kind(),
                        ErrorKind::AddrInUse | ErrorKind::PermissionDenied
                    ) => {}
            Err(error) => return Err(error).context("监听本地 DNS TCP"),
        }
    }
    unreachable!("the last bind attempt returns its error")
}

pub(crate) async fn exchange(
    group: &ProxyGroup,
    upstream: SocketAddrV4,
    query: &[u8],
    meter: Option<&TrafficMeter>,
) -> Result<Vec<u8>> {
    let request = Message::from_vec(query).context("DNS 查询格式无效")?;
    if query.len() > u16::MAX as usize
        || request.message_type() != MessageType::Query
        || request.op_code() != OpCode::Query
        || request.queries().len() != 1
    {
        bail!("DNS 查询格式无效");
    }
    timeout(Duration::from_secs(8), async {
        let mut stream = socks::connect(group, SocketAddr::V4(upstream)).await?;
        stream.write_u16(query.len() as u16).await?;
        if let Some(meter) = meter {
            meter
                .write_all(&mut stream, query, Direction::Upload)
                .await?;
        } else {
            stream.write_all(query).await?;
        }
        let length = stream.read_u16().await?;
        let mut response = vec![0; length as usize];
        stream.read_exact(&mut response).await?;
        let answer = Message::from_vec(&response).context("DNS 上游响应格式无效")?;
        if answer.id() != request.id()
            || answer.message_type() != MessageType::Response
            || answer.op_code() != request.op_code()
            || answer.queries() != request.queries()
        {
            bail!("DNS 上游返回了不匹配的响应");
        }
        Ok(response)
    })
    .await
    .context("DNS 转发超时")?
}
pub(crate) fn udp_response(response: Vec<u8>) -> Result<Vec<u8>> {
    // A small TC reply also works for clients without EDNS; they retry over TCP.
    if response.len() <= 512 {
        return Ok(response);
    }
    let mut truncated = Message::from_vec(&response)?.truncate();
    *truncated.extensions_mut() = None;
    Ok(truncated.to_vec()?)
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
        let response = exchange(&group, upstream, &query, None).await?;
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
        let (tcp, udp) = bind_listeners(listen).await?;
        let SocketAddr::V4(address) = tcp.local_addr()? else {
            bail!("DNS 监听地址必须为 IPv4");
        };
        let udp = Arc::new(udp);
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
                            let response = udp_response(exchange(&group,upstream,&query,None).await?)?;
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
