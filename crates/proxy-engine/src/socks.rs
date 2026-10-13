use anyhow::{Context, Result, bail};
use proxy_core::{config::ProxyGroup, model::ProxyTestResult};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket, lookup_host},
    time::timeout,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocksAddress {
    Ip(SocketAddr),
    Domain(String, u16),
}

pub fn encode_address(address: &SocksAddress, output: &mut Vec<u8>) -> Result<()> {
    let port = match address {
        SocksAddress::Ip(SocketAddr::V4(address)) => {
            output.push(1);
            output.extend(address.ip().octets());
            address.port()
        }
        SocksAddress::Ip(SocketAddr::V6(address)) => {
            output.push(4);
            output.extend(address.ip().octets());
            address.port()
        }
        SocksAddress::Domain(host, port) => {
            if host.is_empty() || host.len() > 255 {
                bail!("SOCKS 主机名长度无效");
            }
            output.extend([3, host.len() as u8]);
            output.extend(host.as_bytes());
            *port
        }
    };
    output.extend(port.to_be_bytes());
    Ok(())
}

pub fn decode_address(bytes: &[u8]) -> Result<(SocksAddress, usize)> {
    let (length, host) = match bytes.first() {
        Some(1) if bytes.len() >= 7 => (
            7,
            Some(IpAddr::V4(Ipv4Addr::new(
                bytes[1], bytes[2], bytes[3], bytes[4],
            ))),
        ),
        Some(4) if bytes.len() >= 19 => (
            19,
            Some(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(
                &bytes[1..17],
            )?))),
        ),
        Some(3) if bytes.len() >= 2 && bytes[1] > 0 && bytes.len() >= bytes[1] as usize + 4 => {
            (bytes[1] as usize + 4, None)
        }
        _ => bail!("SOCKS 地址类型或长度无效"),
    };
    let port = u16::from_be_bytes([bytes[length - 2], bytes[length - 1]]);
    let address = match host {
        Some(ip) => SocksAddress::Ip(SocketAddr::new(ip, port)),
        None => SocksAddress::Domain(std::str::from_utf8(&bytes[2..length - 2])?.into(), port),
    };
    Ok((address, length))
}

async fn greet(group: &ProxyGroup) -> Result<TcpStream> {
    let mut stream = TcpStream::connect((group.host.as_str(), group.port))
        .await
        .with_context(|| format!("连接代理 {}:{}", group.host, group.port))?;
    stream.set_nodelay(true)?;
    stream.write_all(&[5, 1, 0]).await?;
    let mut method = [0; 2];
    stream.read_exact(&mut method).await?;
    if method != [5, 0] {
        bail!("代理不接受 SOCKS5 无认证连接（返回 {method:?}）");
    }
    Ok(stream)
}

async fn request(
    stream: &mut TcpStream,
    command: u8,
    destination: &SocksAddress,
) -> Result<SocksAddress> {
    let mut bytes = vec![5, command, 0];
    encode_address(destination, &mut bytes)?;
    stream.write_all(&bytes).await?;
    let mut header = [0; 4];
    stream.read_exact(&mut header).await?;
    if header[0] != 5 || header[2] != 0 {
        bail!("代理返回了无效的 SOCKS5 响应");
    }
    if header[1] != 0 {
        bail!("SOCKS5 请求被拒绝，错误码 {}", header[1]);
    }
    let mut address = vec![header[3]];
    let length = match header[3] {
        1 => 6,
        4 => 18,
        3 => {
            let length = stream.read_u8().await?;
            address.push(length);
            length as usize + 2
        }
        _ => bail!("代理返回了无效的地址类型"),
    };
    let start = address.len();
    address.resize(start + length, 0);
    stream.read_exact(&mut address[start..]).await?;
    Ok(decode_address(&address)?.0)
}

pub async fn connect(group: &ProxyGroup, destination: SocketAddr) -> Result<TcpStream> {
    timeout(HANDSHAKE_TIMEOUT, async {
        let mut stream = greet(group).await?;
        request(&mut stream, 1, &SocksAddress::Ip(destination)).await?;
        Ok(stream)
    })
    .await
    .context("SOCKS5 CONNECT 超时")?
}

pub async fn test(group: &ProxyGroup) -> Result<ProxyTestResult> {
    let start = Instant::now();
    timeout(HANDSHAKE_TIMEOUT, greet(group))
        .await
        .context("SOCKS5 握手超时")??;
    Ok(ProxyTestResult {
        latency_ms: start.elapsed().as_millis().min(u32::MAX as u128) as u32,
        message: "SOCKS5 握手成功".into(),
    })
}

pub struct Association {
    pub control: TcpStream,
    pub socket: UdpSocket,
}
impl Association {
    pub async fn open(group: &ProxyGroup) -> Result<Self> {
        timeout(HANDSHAKE_TIMEOUT, async {
            let mut control = greet(group).await?;
            let peer = control.peer_addr()?;
            let socket = UdpSocket::bind(if peer.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .await?;
            let relay = request(&mut control, 3, &SocksAddress::Ip(socket.local_addr()?)).await?;
            let mut relay = match relay {
                SocksAddress::Ip(address) => address,
                SocksAddress::Domain(host, port) => lookup_host((host.as_str(), port))
                    .await?
                    .find(|a| a.is_ipv4() == peer.is_ipv4())
                    .context("无法解析 UDP relay")?,
            };
            if relay.ip().is_unspecified() {
                relay.set_ip(peer.ip());
            }
            if relay.port() == 0 {
                bail!("代理返回了无效的 UDP relay 端口");
            }
            socket.connect(relay).await?;
            Ok(Self { control, socket })
        })
        .await
        .context("SOCKS5 UDP ASSOCIATE 超时")?
    }
}

pub fn encode_datagram(destination: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = vec![0, 0, 0];
    encode_address(&SocksAddress::Ip(destination), &mut bytes)?;
    bytes.extend(payload);
    if bytes.len() > 65507 {
        bail!("UDP 数据报超过 IPv4 长度限制");
    }
    Ok(bytes)
}
pub fn decode_datagram(bytes: &[u8]) -> Result<(SocksAddress, &[u8])> {
    if bytes.len() < 4 || bytes[..3] != [0, 0, 0] {
        bail!("SOCKS UDP 头无效或包含不支持的分片");
    }
    let (source, length) = decode_address(&bytes[3..])?;
    Ok((source, &bytes[3 + length..]))
}
