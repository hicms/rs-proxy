use crate::{
    capture::Capture,
    relay,
    state::{Counters, Shared, State},
};
use anyhow::{Context, Result, bail};
use proxy_core::{
    config::Config,
    model::{EngineStats, ProcessIdentity, ReconnectResult, Snapshot},
};
use proxy_platform::{
    connections,
    events::{ProcessEvent, ProcessEvents},
    process,
    windivert::{DivertHandle, Driver},
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_util::sync::CancellationToken;

pub struct Engine {
    shared: Arc<Shared>,
    network: Arc<DivertHandle>,
    socket: Arc<DivertHandle>,
    cancel: CancellationToken,
    workers: Vec<JoinHandle<()>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Engine {
    pub async fn start(config: Config, driver_path: &Path) -> Result<Self> {
        if !proxy_platform::is_elevated() {
            bail!("透明代理需要管理员权限，请以管理员身份运行 RS Proxy");
        }
        let driver = Driver::load(driver_path)?;
        let events = ProcessEvents::start()?;
        let mut state = State::new(config)?;
        state.seed_udp(&connections::snapshot()?);
        let shared = Arc::new(Shared {
            traffic: state.traffic.clone(),
            state: Mutex::new(state),
            counters: Counters::default(),
            stopping: AtomicBool::new(false),
            last_error: Mutex::new(None),
        });
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("启动本地 TCP 转发器")?;
        let tcp_port = listener.local_addr()?.port();
        let socket = driver.open(
            "(tcp and (event == CONNECT or event == CLOSE)) or udp",
            3,
            0,
            1 | 4,
        )?;
        let network = driver.open(&format!("outbound and ip and ((!loopback and (tcp or udp)) or (loopback and tcp.SrcPort == {tcp_port}))"), 0, 0, 0)?;
        network.set_queue(8192, 1000)?;
        let cancel = CancellationToken::new();
        let (udp_tx, udp_rx) = mpsc::channel(4096);
        let capture = Arc::new(Capture::new(
            shared.clone(),
            network.clone(),
            tcp_port,
            udp_tx,
        ));
        let mut engine = Self {
            shared: shared.clone(),
            network: network.clone(),
            socket: socket.clone(),
            cancel: cancel.clone(),
            workers: Vec::new(),
            tasks: Vec::new(),
        };
        let receiver = capture.clone();
        engine.workers.push(
            thread::Builder::new()
                .name("packet-capture".into())
                .spawn(move || receiver.receive_loop())?,
        );
        engine.workers.push(
            thread::Builder::new()
                .name("syn-watchdog".into())
                .spawn(move || capture.watchdog_loop())?,
        );
        let owner = shared.clone();
        let network_stop = network.clone();
        engine.workers.push(
            thread::Builder::new()
                .name("socket-decisions".into())
                .spawn(move || {
                    loop {
                        let event = match socket.receive(&mut []) {
                            Ok((_, address)) => address.socket_event(),
                            Err(error) => {
                                if !owner.stopping.load(Ordering::Acquire) {
                                    owner.error(format!("socket 事件线程结束：{error}"));
                                    owner.stopping.store(true, Ordering::Release);
                                    let _ = network_stop.shutdown_receive();
                                }
                                break;
                            }
                        };
                        if let Some(event) = event {
                            let ancestors = if event.kind == 7 {
                                Vec::new()
                            } else {
                                process::query_socket_ancestors(event.pid, event.timestamp)
                                    .unwrap_or_default()
                            };
                            owner.state.lock().unwrap().socket_event(event, ancestors);
                        }
                    }
                })?,
        );
        let owner = shared.clone();
        engine
            .workers
            .push(
                thread::Builder::new()
                    .name("process-tree".into())
                    .spawn(move || {
                        let mut last_sync = Instant::now();
                        while !owner.stopping.load(Ordering::Acquire) {
                            if let Ok(event) =
                                events.receiver.recv_timeout(Duration::from_millis(50))
                            {
                                let mut state = owner.state.lock().unwrap();
                                match event {
                                    ProcessEvent::Started(process) => state.tree.upsert(process),
                                    ProcessEvent::Exited(identity) => state.tree.exit(&identity),
                                }
                            }
                            if events.take_sync_required()
                                || last_sync.elapsed() >= Duration::from_secs(30)
                            {
                                match process::snapshot() {
                                    Ok(processes) => {
                                        owner.state.lock().unwrap().tree.synchronize(processes)
                                    }
                                    Err(error) => owner.error(error),
                                }
                                last_sync = Instant::now();
                            }
                        }
                    })?,
            );
        let supervisor_shared = shared.clone();
        let supervisor_network = network.clone();
        let supervisor_socket = engine.socket.clone();
        let supervisor_cancel = cancel.clone();
        engine.tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = supervisor_cancel.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if supervisor_shared.stopping.load(Ordering::Acquire) { break; }
                    }
                }
            }
            supervisor_shared.stopping.store(true, Ordering::Release);
            supervisor_cancel.cancel();
            let _ = supervisor_network.shutdown_receive();
            let _ = supervisor_socket.shutdown_receive();
        }));
        let sampler = shared.clone();
        let sampler_cancel = cancel.clone();
        engine.tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = sampler_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let state = sampler.state.lock().unwrap();
                        sampler.traffic.sample(Instant::now(), &state.tree);
                    }
                }
            }
        }));
        engine.tasks.push(tokio::spawn(relay::tcp_loop(
            listener,
            shared.clone(),
            cancel.clone(),
        )));
        engine.tasks.push(tokio::spawn(relay::udp_loop(
            udp_rx, shared, network, cancel,
        )));
        Ok(engine)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let connections = connections::snapshot()?;
        let (config, tree, policy) = {
            let state = self.shared.state.lock().unwrap();
            (
                state.config.clone(),
                state.tree.clone(),
                state.policy.clone(),
            )
        };
        let stats = self.shared.stats();
        let traffic = self.shared.traffic.snapshot(stats.running);
        Ok(Snapshot {
            config,
            processes: tree.snapshot(&policy),
            connections,
            stats,
            traffic,
        })
    }
    pub fn stats(&self) -> EngineStats {
        self.shared.stats()
    }
    pub fn update_config(&self, config: Config, replacement: Option<(u32, u32)>) -> Result<()> {
        self.shared
            .state
            .lock()
            .unwrap()
            .update(config, replacement)
    }
    pub fn validate_config(&self, config: &Config, replacement: Option<(u32, u32)>) -> Result<()> {
        self.shared
            .state
            .lock()
            .unwrap()
            .validate_update(config, replacement)
    }
    pub fn set_process_proxy(&self, identity: &ProcessIdentity, group: Option<u32>) -> Result<()> {
        self.shared
            .state
            .lock()
            .unwrap()
            .override_process(identity, group)
    }
    pub fn reconnect_processes(
        &self,
        identities: &[ProcessIdentity],
        group_id: Option<u32>,
    ) -> Result<ReconnectResult> {
        crate::reconnect::reconnect(&self.shared, identities, group_id)
    }
    pub async fn shutdown(&mut self) {
        self.begin_shutdown();
        for task in self.tasks.drain(..) {
            if let Err(error) = task.await {
                self.shared.error(error);
            }
        }
        let workers = std::mem::take(&mut self.workers);
        let _ = tokio::task::spawn_blocking(move || {
            for worker in workers {
                let _ = worker.join();
            }
        })
        .await;
    }
    fn begin_shutdown(&self) {
        self.shared.stopping.store(true, Ordering::Release);
        self.cancel.cancel();
        if let Err(error) = self.socket.shutdown_receive() {
            self.shared.error(error);
        }
        if let Err(error) = self.network.shutdown_receive() {
            self.shared.error(error);
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        if !self.workers.is_empty() {
            self.begin_shutdown();
            for worker in self.workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
}
