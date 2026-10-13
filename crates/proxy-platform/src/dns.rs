use crate::windows::{status, wide, wide_string};
use anyhow::{Context, Result, bail};
use proxy_core::store::atomic_write;
use serde::{Deserialize, Serialize};
use std::{ffi::CStr, fs, path::PathBuf, ptr};
use windows_sys::{
    Win32::{
        Foundation::ERROR_BUFFER_OVERFLOW,
        NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
        Networking::WinSock::AF_INET,
    },
    core::GUID,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterDns {
    pub id: String,
    pub name: String,
    pub servers: String,
}

pub trait DnsBackend {
    fn adapters(&self) -> Result<Vec<AdapterDns>>;
    fn read(&self, id: &str) -> Result<String>;
    fn set(&self, id: &str, servers: &str) -> Result<()>;
}

fn guid(id: &str) -> Result<GUID> {
    let hex: String = id
        .chars()
        .filter(|c| !matches!(c, '{' | '}' | '-'))
        .collect();
    if hex.len() != 32 {
        bail!("无效的网络接口 ID");
    }
    Ok(GUID::from_u128(
        u128::from_str_radix(&hex, 16).context("网络接口 ID 不是十六进制 GUID")?,
    ))
}
fn same_servers(a: &str, b: &str) -> bool {
    let list = |value: &str| {
        value
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    list(a) == list(b)
}

#[derive(Default)]
pub struct WindowsDns;
impl DnsBackend for WindowsDns {
    fn adapters(&self) -> Result<Vec<AdapterDns>> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size = 16384;
        loop {
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            let code = unsafe {
                GetAdaptersAddresses(AF_INET as u32, flags, ptr::null(), first, &mut size)
            };
            if code == ERROR_BUFFER_OVERFLOW {
                continue;
            }
            status(code).context("枚举 IPv4 网络接口")?;
            let mut result = Vec::new();
            let mut cursor = first;
            while !cursor.is_null() {
                let adapter = unsafe { &*cursor };
                if adapter.OperStatus == IfOperStatusUp
                    && adapter.IfType != 24
                    && !adapter.FirstUnicastAddress.is_null()
                {
                    let id = unsafe { CStr::from_ptr(adapter.AdapterName.cast()) }
                        .to_string_lossy()
                        .into_owned();
                    result.push(AdapterDns {
                        servers: self.read(&id)?,
                        id,
                        name: unsafe { wide_string(adapter.FriendlyName) },
                    });
                }
                cursor = adapter.Next;
            }
            return Ok(result);
        }
    }
    fn read(&self, id: &str) -> Result<String> {
        let mut settings = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            ..Default::default()
        };
        status(unsafe { GetInterfaceDnsSettings(guid(id)?, &mut settings) })
            .with_context(|| format!("读取接口 {id} 的 DNS"))?;
        // NameServer describes static servers; an empty string preserves automatic DNS.
        let servers = unsafe { wide_string(settings.NameServer) };
        unsafe {
            FreeInterfaceDnsSettings(&mut settings);
        }
        Ok(servers)
    }
    fn set(&self, id: &str, servers: &str) -> Result<()> {
        let mut value = wide(servers);
        let settings = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            Flags: DNS_SETTING_NAMESERVER as u64,
            NameServer: value.as_mut_ptr(),
            ..Default::default()
        };
        status(unsafe { SetInterfaceDnsSettings(guid(id)?, &settings) })
            .with_context(|| format!("设置接口 {id} 的 DNS"))?;
        if !same_servers(&self.read(id)?, servers) {
            bail!("接口 {id} 的 DNS 设置未生效");
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Journal {
    version: u32,
    adapters: Vec<AdapterDns>,
}

pub struct DnsController<B: DnsBackend = WindowsDns> {
    backend: B,
    journal_path: PathBuf,
    active: bool,
}
impl<B: DnsBackend> DnsController<B> {
    pub fn new(journal_path: impl Into<PathBuf>, backend: B) -> Self {
        Self {
            backend,
            journal_path: journal_path.into(),
            active: false,
        }
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn enable(&mut self, listen: &str) -> Result<()> {
        let listen: std::net::Ipv4Addr = listen.parse().context("DNS 监听地址无效")?;
        if !listen.is_loopback() {
            bail!("DNS 转发只允许回环地址");
        }
        self.restore()?;
        let adapters = self.backend.adapters()?;
        if adapters.is_empty() {
            bail!("没有可配置 DNS 的活动 IPv4 接口");
        }
        let journal = Journal {
            version: 1,
            adapters,
        };
        self.write(&journal)?;
        self.active = true;
        for adapter in &journal.adapters {
            if let Err(error) = self.backend.set(&adapter.id, &listen.to_string()) {
                return match self.restore() {
                    Ok(()) => Err(error.context("DNS 启用失败，原设置已恢复")),
                    Err(restore) => {
                        Err(error.context(format!("DNS 启用失败，恢复尚未完成：{restore:#}")))
                    }
                };
            }
        }
        self.active = true;
        Ok(())
    }
    pub fn restore(&mut self) -> Result<()> {
        let bytes = match fs::read(&self.journal_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.active = false;
                return Ok(());
            }
            Err(error) => return Err(error).context("读取 DNS 恢复日志"),
        };
        let mut journal: Journal =
            serde_json::from_slice(&bytes).context("DNS 恢复日志格式无效")?;
        if journal.version != 1 {
            bail!("不支持的 DNS 恢复日志版本");
        }
        self.active = !journal.adapters.is_empty();
        let mut errors = Vec::new();
        for index in (0..journal.adapters.len()).rev() {
            let adapter = &journal.adapters[index];
            match self.backend.set(&adapter.id, &adapter.servers) {
                Ok(()) => {
                    journal.adapters.remove(index);
                    self.write(&journal)?;
                }
                Err(error) => errors.push(format!("{}：{error:#}", adapter.name)),
            }
        }
        if !errors.is_empty() {
            bail!("部分 DNS 尚未恢复，已保留恢复日志：{}", errors.join("；"));
        }
        fs::remove_file(&self.journal_path).context("清理已完成的 DNS 恢复日志")?;
        self.active = false;
        Ok(())
    }
    fn write(&self, journal: &Journal) -> Result<()> {
        atomic_write(&self.journal_path, &serde_json::to_vec_pretty(journal)?)
            .context("持久化 DNS 恢复日志")
    }
}
