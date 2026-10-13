use anyhow::{Context, Result, ensure};
use proxy_core::config::Config;
use proxy_engine::Engine;
use std::{
    net::SocketAddrV4,
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::Command,
    task::JoinSet,
    time::timeout,
};
use tokio_util::sync::CancellationToken;

async fn serve(mut socket: TcpStream, destinations: Arc<Mutex<Vec<SocketAddrV4>>>) -> Result<()> {
    let mut hello = [0; 3];
    socket.read_exact(&mut hello).await?;
    ensure!(hello == [5, 1, 0], "unexpected greeting");
    socket.write_all(&[5, 0]).await?;
    let mut request = [0; 10];
    socket.read_exact(&mut request).await?;
    ensure!(
        request[..4] == [5, 1, 0, 1],
        "DNS requires only SOCKS TCP CONNECT"
    );
    destinations.lock().unwrap().push(SocketAddrV4::new(
        [request[4], request[5], request[6], request[7]].into(),
        u16::from_be_bytes([request[8], request[9]]),
    ));
    socket.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?;
    let length = socket.read_u16().await?;
    let mut answer = vec![0; length as usize];
    socket.read_exact(&mut answer).await?;
    let name = String::from_utf8_lossy(&answer).to_string();
    if name.contains("slow") {
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    if name.contains("invalid") {
        answer[13] ^= 1;
    }
    answer[2] |= 0x80;
    let count = if name.contains("large") { 40 } else { 1 };
    answer[7] = count;
    for _ in 0..count {
        answer.extend([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 198, 19, 99, 9]);
    }
    socket.write_u16(answer.len() as u16).await?;
    socket.write_all(&answer).await?;
    Ok(())
}

async fn client(
    engine: &Engine,
    executable: &Path,
    mode: &str,
    destination: &str,
    name: &str,
) -> Result<serde_json::Value> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/dns-client.mjs");
    let mut child = Command::new(executable)
        .args([script.to_str().unwrap(), mode, destination, name])
        .creation_flags(0x08000000)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id().context("missing fixture PID")?;
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut ready = String::new();
    timeout(Duration::from_secs(10), output.read_line(&mut ready)).await??;
    ensure!(ready.trim() == "READY", "fixture not ready");
    engine.set_process_proxy(&proxy_platform::process::query(pid)?.identity, Some(0))?;
    child.stdin.take().unwrap().write_all(b"GO\n").await?;
    let mut result = String::new();
    timeout(Duration::from_secs(10), output.read_to_string(&mut result)).await??;
    let status = child.wait().await?;
    let mut errors = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .await?;
    ensure!(
        status.success(),
        "DNS client failed: {errors}; stats: {:?}",
        engine.stats()
    );
    Ok(serde_json::from_str(result.trim())?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selected_process_dns_uses_its_proxy_and_configured_resolver() -> Result<()> {
    ensure!(
        !proxy_platform::process::snapshot()?
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("rs-proxy.exe")),
        "Close RS Proxy before controlled capture tests"
    );
    let temporary = tempfile::tempdir()?;
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()?;
    ensure!(
        node.status.success(),
        "Node.js is required for this opt-in test"
    );
    let executable = temporary.path().join("rs-dns-fixture.exe");
    std::fs::copy(String::from_utf8(node.stdout)?.trim(), &executable)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut config = Config::default();
    config.proxy_groups[0].port = listener.local_addr()?.port();
    config.dns.upstream_host = "1.1.1.1".into();
    config.dns.upstream_port = 5353;
    config.proxy_groups.push(proxy_core::config::ProxyGroup {
        id: 99,
        port: 1,
        ..Default::default()
    });
    config.dns.proxy_group_id = 99;
    ensure!(!config.dns.enabled, "system DNS must remain disabled");
    let destinations = Arc::new(Mutex::new(Vec::new()));
    let recorded = destinations.clone();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                _ = stopped.cancelled() => break,
                Some(result) = tasks.join_next(), if !tasks.is_empty() => { result??; },
                accepted = listener.accept() => { let (socket,_) = accepted?; tasks.spawn(serve(socket,recorded.clone())); }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        Ok::<_, anyhow::Error>(())
    });
    let mut engine = Engine::start(
        config.clone(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/windivert"),
    )
    .await?;
    let result = async {
        client(&engine,&executable,"tcp","198.19.99.53","one.test").await?;
        ensure!(destinations.lock().unwrap().as_slice() == ["1.1.1.1:5353".parse::<SocketAddrV4>()?],"selected TCP DNS used the original resolver instead of the configured proxy resolver: {:?}",destinations.lock().unwrap());
        let before = engine.stats();
        let (first, second) = tokio::try_join!(client(&engine,&executable,"udp","192.168.250.53","one.test"), client(&engine,&executable,"udp","198.19.99.53","two.test"))?;
        let after = engine.stats();
        ensure!(after.uploaded_bytes - before.uploaded_bytes == first["queryBytes"].as_f64().unwrap() + second["queryBytes"].as_f64().unwrap(),"UDP DNS uploads must exclude TCP framing");
        ensure!(after.downloaded_bytes - before.downloaded_bytes == first["bytes"].as_f64().unwrap() + second["bytes"].as_f64().unwrap(),"UDP DNS download count");
        ensure!(destinations.lock().unwrap().iter().all(|address| address.to_string() == "1.1.1.1:5353"),"DNS destination mismatch");
        let before = engine.stats();
        let large = client(&engine,&executable,"retry","192.168.250.53","large.test").await?;
        let after = engine.stats();
        ensure!(after.uploaded_bytes - before.uploaded_bytes == large["queryBytes"].as_f64().unwrap() * 2.0 + 2.0,"TCP retry upload count");
        ensure!(after.downloaded_bytes - before.downloaded_bytes == large["bytes"].as_f64().unwrap() + large["truncatedBytes"].as_f64().unwrap() + 2.0,"TC and full TCP download count");
        for (mode,name) in [("silent","invalid.test"),("rebind","slow.test")] {
            let before = engine.stats();
            let ignored = client(&engine,&executable,mode,"192.168.250.53",name).await?;
            let after = engine.stats();
            ensure!(after.uploaded_bytes - before.uploaded_bytes == ignored["queryBytes"].as_f64().unwrap(),"completed writes must count even when the query fails");
            ensure!(after.downloaded_bytes == before.downloaded_bytes,"invalid/stale reply must not be injected or counted");
        }
        let count = destinations.lock().unwrap().len();
        let in_flight = client(&engine,&executable,"udp","192.168.250.53","slow.test");
        let change = async {
            timeout(Duration::from_secs(5),async { while destinations.lock().unwrap().len() == count { tokio::time::sleep(Duration::from_millis(10)).await; } }).await?;
            config.dns.upstream_host = "9.9.9.9".into(); config.dns.upstream_port = 54;
            engine.update_config(config.clone(),None)?;
            Ok::<_,anyhow::Error>(())
        };
        tokio::try_join!(in_flight,change)?;
        ensure!(destinations.lock().unwrap()[count].to_string() == "1.1.1.1:5353","in-flight target changed");
        client(&engine,&executable,"udp","192.168.250.53","next.test").await?;
        ensure!(destinations.lock().unwrap().last().unwrap().to_string() == "9.9.9.9:54","new query did not use updated resolver");
        config.proxy_process_dns = false; engine.update_config(config,None)?;
        client(&engine,&executable,"tcp","198.19.99.53","plain.test").await?;
        ensure!(destinations.lock().unwrap().last().unwrap().to_string() == "198.19.99.53:53","disabled mapping must keep original TCP target");
        println!("TCP/UDP DNS, same-ID clients, TCP-only SOCKS, TC retry, traffic accounting, invalid/stale replies and hot configuration passed");
        Ok(())
    }.await;
    engine.shutdown().await;
    stop.cancel();
    timeout(Duration::from_secs(3), server).await???;
    result
}
