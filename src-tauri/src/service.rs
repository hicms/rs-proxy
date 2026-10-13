use anyhow::{Context, Result, bail};
use proxy_core::{
    config::Config,
    model::{EngineStats, ProcessIdentity, ReconnectResult, Snapshot},
    rules::RoutingPolicy,
    store::ConfigStore,
    tree::ProcessTree,
};
use proxy_engine::{Engine, dns::DnsForwarder};
use proxy_platform::{
    connections,
    dns::{DnsController, WindowsDns},
    process,
};
use std::path::{Path, PathBuf};

pub struct Service {
    store: ConfigStore,
    engine: Option<Engine>,
    dns: DnsController,
    forwarder: Option<DnsForwarder>,
    driver_path: PathBuf,
    error: Option<String>,
}
impl Service {
    pub fn open(data: &Path, driver_path: PathBuf) -> Result<Self> {
        let mut dns = DnsController::new(data.join("dns-recovery.json"), WindowsDns);
        dns.restore().context("恢复系统 DNS")?;
        Ok(Self {
            store: ConfigStore::open(data.join("config.json"))?,
            engine: None,
            dns,
            forwarder: None,
            driver_path,
            error: None,
        })
    }
    pub fn config(&self) -> &Config {
        self.store.config()
    }
    pub async fn start(&mut self) -> Result<()> {
        if self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.stats().running)
        {
            return Ok(());
        }
        if self.engine.is_some() {
            self.stop().await?;
        }
        let engine = match Engine::start(self.config().clone(), &self.driver_path).await {
            Ok(engine) => engine,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                return Err(error);
            }
        };
        self.engine = Some(engine);
        if let Err(error) = self.configure_dns(&self.config().clone()).await {
            self.error = Some(format!("{error:#}"));
            return Err(error);
        }
        self.error = None;
        Ok(())
    }
    async fn stop_dns(&mut self) -> Result<()> {
        // Keep the listener alive whenever any adapter still needs it.
        self.dns.restore()?;
        if let Some(mut forwarder) = self.forwarder.take() {
            forwarder.shutdown().await;
        }
        Ok(())
    }
    async fn configure_dns(&mut self, config: &Config) -> Result<()> {
        self.stop_dns().await?;
        if !config.dns.enabled || self.engine.is_none() {
            return Ok(());
        }
        let dns = &config.dns;
        let group = config
            .proxy_groups
            .iter()
            .find(|g| g.id == dns.proxy_group_id)
            .context("DNS 代理组不存在")?
            .clone();
        self.forwarder = Some(
            DnsForwarder::start(
                format!("{}:{}", dns.listen_host, dns.listen_port).parse()?,
                format!("{}:{}", dns.upstream_host, dns.upstream_port).parse()?,
                group,
            )
            .await?,
        );
        self.dns.enable(&dns.listen_host)?;
        Ok(())
    }
    pub async fn save(&mut self, config: Config) -> Result<Config> {
        self.save_internal(config, None).await
    }
    async fn save_internal(
        &mut self,
        config: Config,
        replacement: Option<(u32, u32)>,
    ) -> Result<Config> {
        config.validate()?;
        let old = self.config().clone();
        if config.revision != old.revision {
            bail!("配置已被更新，请刷新后再保存");
        }
        if let Some(engine) = &self.engine {
            engine.validate_config(&config, replacement)?;
        }
        let dns_changed = config.dns != old.dns || config.proxy_groups != old.proxy_groups;
        if dns_changed && let Err(error) = self.configure_dns(&config).await {
            return Err(self.rollback_dns(&old, error).await);
        }
        let saved = match self.store.replace(config) {
            Ok(saved) => saved,
            Err(error) => {
                let error = error.into();
                return Err(if dns_changed {
                    self.rollback_dns(&old, error).await
                } else {
                    error
                });
            }
        };
        if let Some(engine) = &self.engine {
            engine.update_config(saved.clone(), replacement)?;
        }
        self.error = None;
        Ok(saved)
    }
    async fn rollback_dns(&mut self, old: &Config, error: anyhow::Error) -> anyhow::Error {
        let error = match self.configure_dns(old).await {
            Ok(()) => error,
            Err(restore) => error.context(format!("恢复 DNS 状态失败：{restore:#}")),
        };
        self.error = Some(format!("{error:#}"));
        error
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let mut snapshot = if let Some(engine) = &self.engine {
            engine.snapshot()?
        } else {
            let config = self.config().clone();
            let policy = RoutingPolicy::new(&config)?;
            let mut tree = ProcessTree::default();
            tree.synchronize(process::snapshot()?);
            Snapshot {
                traffic: Default::default(),
                config,
                processes: tree.snapshot(&policy),
                connections: connections::snapshot()?,
                stats: EngineStats {
                    elevated: proxy_platform::is_elevated(),
                    ..Default::default()
                },
            }
        };
        snapshot.stats.dns_active = self.dns.active();
        if self.error.is_some() {
            snapshot.stats.last_error = self.error.clone();
        }
        Ok(snapshot)
    }
    pub fn set_process_proxy(&self, identity: &ProcessIdentity, group: Option<u32>) -> Result<()> {
        self.engine
            .as_ref()
            .context("请先启动代理引擎")?
            .set_process_proxy(identity, group)
    }
    pub fn reconnect_processes(
        &self,
        identities: &[ProcessIdentity],
        group_id: Option<u32>,
    ) -> Result<ReconnectResult> {
        self.engine
            .as_ref()
            .context("请先启动代理引擎")?
            .reconnect_processes(identities, group_id)
    }
    pub async fn delete_group(&mut self, from: u32, to: u32, revision: u32) -> Result<Config> {
        let mut config = self.config().clone();
        if from == to
            || !config.proxy_groups.iter().any(|g| g.id == from)
            || !config.proxy_groups.iter().any(|g| g.id == to)
        {
            bail!("代理组不存在或替换目标无效");
        }
        config.revision = revision;
        config.proxy_groups.retain(|group| group.id != from);
        for rule in &mut config.auto_rules {
            if rule.proxy_group_id == from {
                rule.proxy_group_id = to;
            }
        }
        if config.dns.proxy_group_id == from {
            config.dns.proxy_group_id = to;
        }
        self.save_internal(config, Some((from, to))).await
    }
    pub async fn stop(&mut self) -> Result<()> {
        if let Err(error) = self.stop_dns().await {
            self.error = Some(format!("{error:#}"));
            return Err(error);
        }
        if let Some(mut engine) = self.engine.take() {
            engine.shutdown().await;
        }
        self.error = None;
        Ok(())
    }
}
