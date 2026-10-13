use anyhow::{Context, Result, bail};
use proxy_core::config::Config;
use proxy_engine::Engine;
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream, UdpSocket},
    process::Command,
    task::JoinSet,
    time::timeout,
};
use tokio_util::sync::CancellationToken;
mod support;

// Zero-timeout linger sends RST immediately; it cannot block like positive linger timeouts.
#[allow(deprecated)]
fn reset_connection(stream: TcpStream) -> Result<()> {
    stream.set_linger(Some(Duration::ZERO))?;
    Ok(())
}

async fn mock_connection(
    mut stream: TcpStream,
    targets: Arc<Mutex<Vec<(u8, SocketAddrV4)>>>,
    stop: CancellationToken,
) -> Result<()> {
    let mut hello = [0; 3];
    stream.read_exact(&mut hello).await?;
    if hello != [5, 1, 0] {
        bail!("invalid greeting");
    }
    stream.write_all(&[5, 0]).await?;
    let mut request = [0; 10];
    stream.read_exact(&mut request).await?;
    if request[0] != 5 || request[2..4] != [0, 1] {
        bail!("invalid request");
    }
    let remote = SocketAddrV4::new(
        Ipv4Addr::new(request[4], request[5], request[6], request[7]),
        u16::from_be_bytes([request[8], request[9]]),
    );
    targets.lock().unwrap().push((request[1], remote));
    if remote.ip() == &Ipv4Addr::new(198, 19, 99, 7) {
        return reset_connection(stream);
    }
    if remote.ip() == &Ipv4Addr::new(198, 19, 99, 5) {
        stream.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Ok(());
    }
    if request[1] == 1 {
        stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?;
        let mut payload = [0; 9];
        stream.read_exact(&mut payload).await?;
        if &payload != b"tcp-proof" {
            bail!("unexpected TCP payload");
        }
        if remote.ip() == &Ipv4Addr::new(198, 19, 99, 6) {
            return reset_connection(stream);
        }
        stream.write_all(&payload).await?;
        stream.shutdown().await?;
    } else if request[1] == 3 {
        let relay = UdpSocket::bind("127.0.0.1:0").await?;
        let port = relay.local_addr()?.port().to_be_bytes();
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, port[0], port[1]])
            .await?;
        let mut bytes = vec![0u8; 65535];
        let mut byte = [0u8; 1];
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = stream.read(&mut byte) => break,
                received = relay.recv_from(&mut bytes) => {
                    let (length,peer) = received?;
                    if length < 10 || bytes[..4] != [0,0,0,1] { bail!("invalid UDP payload"); }
                    targets.lock().unwrap().push((17,SocketAddrV4::new(Ipv4Addr::new(bytes[4],bytes[5],bytes[6],bytes[7]),u16::from_be_bytes([bytes[8],bytes[9]]))));
                    if &bytes[10..length] == b"close-control" { stream.shutdown().await?; break; }
                    relay.send_to(&bytes[..length],peer).await?;
                }
            }
        }
    } else {
        bail!("unsupported command");
    }
    Ok(())
}

async fn run_client(engine: &Engine, script: &str, selected: bool) -> Result<String> {
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id().context("missing child PID")?;
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut ready = String::new();
    timeout(Duration::from_secs(10), output.read_line(&mut ready)).await??;
    if ready.trim() != "READY" {
        bail!("client did not initialize: {ready}");
    }
    if selected {
        engine.set_process_proxy(&proxy_platform::process::query(pid)?.identity, Some(0))?;
    }
    child.stdin.take().unwrap().write_all(b"GO\n").await?;
    let mut result = String::new();
    let read = timeout(Duration::from_secs(15), output.read_to_string(&mut result)).await;
    if read.is_err() {
        child.kill().await?;
    }
    let status = child.wait().await?;
    let mut errors = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .await?;
    read.context("client timed out")??;
    if !status.success() {
        bail!("client failed: {errors}; stats: {:?}", engine.stats());
    }
    Ok(result)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selected_process_tcp_and_udp_reach_socks_for_nonloopback_targets() -> Result<()> {
    let direct_peer = support::DirectTcpPeer::start(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/windivert"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut config = Config::default();
    config.proxy_groups[0].port = listener.local_addr()?.port();
    let targets = Arc::new(Mutex::new(Vec::new()));
    let stop = CancellationToken::new();
    let server_stop = stop.clone();
    let recorded = targets.clone();
    let server = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                _ = server_stop.cancelled() => break,
                accepted = listener.accept() => {
                    let (stream,_) = accepted.unwrap(); tasks.spawn(mock_connection(stream,recorded.clone(),server_stop.clone()));
                }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    });
    let mut engine = Engine::start(
        config,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/windivert"),
    )
    .await?;
    let result = async {
        let tcp_script = r#"
$ErrorActionPreference='Stop'; [Console]::WriteLine('READY'); [Console]::ReadLine() | Out-Null
$client=[Net.Sockets.TcpClient]::new(); $client.ReceiveTimeout=5000; $client.SendTimeout=5000
$connect=$client.ConnectAsync('198.19.99.1',443); if (-not $connect.Wait(6000)) { throw 'TCP connect timeout' }
$stream=$client.GetStream(); $data=[Text.Encoding]::ASCII.GetBytes('tcp-proof'); $stream.Write($data,0,$data.Length)
$reply=[byte[]]::new(9); $offset=0; while ($offset -lt 9) { $count=$stream.Read($reply,$offset,9-$offset); if ($count -eq 0) { throw 'EOF' }; $offset+=$count }
$client.Client.Shutdown([Net.Sockets.SocketShutdown]::Send); if ($stream.ReadByte() -ne -1) { throw 'missing EOF' }
[Console]::WriteLine([Text.Encoding]::ASCII.GetString($reply)); $client.Dispose()
"#;
        let tcp = run_client(&engine,tcp_script,true).await?;
        assert_eq!(tcp.trim(),"tcp-proof");
        let reset_script = r#"
$ErrorActionPreference='Stop'; [Console]::WriteLine('READY'); [Console]::ReadLine() | Out-Null
$client=[Net.Sockets.TcpClient]::new(); $client.ReceiveTimeout=3000
$client.Connect('198.19.99.6',443)
$stream=$client.GetStream(); $data=[Text.Encoding]::ASCII.GetBytes('tcp-proof'); $stream.Write($data,0,$data.Length)
try { if ($stream.ReadByte() -ne -1) { throw 'expected closed stream' } }
catch [System.IO.IOException] { if ($_.Exception.InnerException.SocketErrorCode -eq [Net.Sockets.SocketError]::TimedOut) { throw } }
$client.Dispose(); [Console]::WriteLine('reset')
"#;
        assert_eq!(run_client(&engine,reset_script,true).await?.trim(),"reset");
        timeout(Duration::from_secs(3), async {
            while engine.stats().tcp_connections != 0 { tokio::time::sleep(Duration::from_millis(10)).await; }
        }).await?;
        assert!(engine.stats().running);
        assert_eq!(engine.stats().last_error, None, "an established peer reset is not an engine failure");
        assert_eq!(run_client(&engine,tcp_script,true).await?.trim(),"tcp-proof");
        let handshake_reset = reset_script.replace("198.19.99.6", "198.19.99.7")
            .replace("$stream.Write($data,0,$data.Length)", "");
        assert_eq!(run_client(&engine,&handshake_reset,true).await?.trim(),"reset");
        timeout(Duration::from_secs(3), async {
            while engine.stats().last_error.is_none() { tokio::time::sleep(Duration::from_millis(10)).await; }
        }).await?;
        assert!(engine.stats().last_error.unwrap().contains("建立 SOCKS5 连接"));
        let direct_script = tcp_script.replace("198.19.99.1',443", "198.19.99.4',4444");
        assert_eq!(run_client(&engine,&direct_script,false).await?.trim(),"tcp-proof");
        assert!(direct_peer.received.load(std::sync::atomic::Ordering::Acquire));
        let rejected = run_client(&engine,r#"
$ErrorActionPreference='Stop'; [Console]::WriteLine('READY'); [Console]::ReadLine() | Out-Null
$client=[Net.Sockets.TcpClient]::new(); $client.ReceiveTimeout=2000
$client.Connect('198.19.99.5',443)
try { if ($client.GetStream().ReadByte() -ne -1) { throw 'expected EOF' } }
catch [System.IO.IOException] { if ($_.Exception.InnerException.SocketErrorCode -eq [Net.Sockets.SocketError]::TimedOut) { throw } }
$client.Dispose(); [Console]::WriteLine('rejected')
"#,true).await?;
        assert_eq!(rejected.trim(),"rejected");
        assert!(engine.stats().last_error.unwrap().contains("SOCKS5 请求被拒绝"));
        let udp = run_client(&engine,r#"
$ErrorActionPreference='Stop'; [Console]::WriteLine('READY'); [Console]::ReadLine() | Out-Null
$client=[Net.Sockets.UdpClient]::new(0); $client.Client.ReceiveTimeout=5000
$second=[Net.Sockets.UdpClient]::new(0); $second.Client.ReceiveTimeout=5000
foreach ($hostAddress in @('198.19.99.2','198.19.99.3')) {
    $data=[Text.Encoding]::ASCII.GetBytes($hostAddress); $client.Send($data,$data.Length,$hostAddress,5353) | Out-Null
    $from=[Net.IPEndPoint]::new([Net.IPAddress]::Any,0); $reply=$client.Receive([ref]$from)
    if ($from.Address.ToString() -ne $hostAddress -or $from.Port -ne 5353) { throw 'UDP source mismatch' }
    if ([Text.Encoding]::ASCII.GetString($reply) -ne $hostAddress) { throw 'UDP payload mismatch' }
}
$data=[Text.Encoding]::ASCII.GetBytes('second-socket'); $second.Send($data,$data.Length,'198.19.99.2',5353) | Out-Null
$from=[Net.IPEndPoint]::new([Net.IPAddress]::Any,0); $reply=$second.Receive([ref]$from)
if ([Text.Encoding]::ASCII.GetString($reply) -ne 'second-socket') { throw 'UDP isolation failure' }
$client.Dispose(); Start-Sleep -Milliseconds 100
$second.Send($data,$data.Length,'198.19.99.3',5353) | Out-Null; $reply=$second.Receive([ref]$from)
if ([Text.Encoding]::ASCII.GetString($reply) -ne 'second-socket') { throw 'UDP surviving socket failure' }
$data=[Text.Encoding]::ASCII.GetBytes('close-control'); $second.Send($data,$data.Length,'198.19.99.3',5353) | Out-Null
Start-Sleep -Milliseconds 1200
$data=[Text.Encoding]::ASCII.GetBytes('reopened'); $second.Send($data,$data.Length,'198.19.99.3',5353) | Out-Null
$reply=$second.Receive([ref]$from); if ([Text.Encoding]::ASCII.GetString($reply) -ne 'reopened') { throw 'UDP control reconnection failure' }
[Console]::WriteLine('udp-proof'); $second.Dispose()
"#,true).await?;
        assert_eq!(udp.trim(),"udp-proof");
        let targets = targets.lock().unwrap();
        assert!(targets.contains(&(1,"198.19.99.1:443".parse().unwrap())));
        assert!(!targets.contains(&(1,"198.19.99.4:4444".parse().unwrap())));
        assert!(targets.contains(&(17,"198.19.99.2:5353".parse().unwrap())));
        assert!(targets.contains(&(17,"198.19.99.3:5353".parse().unwrap())));
        assert!(targets.iter().filter(|(kind,_)| *kind == 3).count() >= 3);
        assert!(engine.stats().proxied_packets > 0.0);
        Ok(())
    }.await;
    engine.shutdown().await;
    stop.cancel();
    server.await?;
    result
}
