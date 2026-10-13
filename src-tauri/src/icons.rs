use anyhow::{anyhow, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use proxy_core::model::{ProcessIcon, ProcessIdentity};
use proxy_platform::{icons::ExecutableIcons, process};
use std::{path::Path, sync::Mutex};

#[derive(Default)]
pub struct ProcessIcons(Mutex<ExecutableIcons>);

impl ProcessIcons {
    /// Requery identities rather than accepting arbitrary file paths from the webview.
    pub fn get(&self, identities: Vec<ProcessIdentity>) -> anyhow::Result<Vec<ProcessIcon>> {
        ensure!(identities.len() <= 50, "一次最多读取50个进程图标");
        let mut cache = self.0.lock().map_err(|_| anyhow!("图标缓存不可用"))?;
        Ok(identities
            .into_iter()
            .map(|identity| {
                let data_url = process::query(identity.pid)
                    .ok()
                    .filter(|info| info.identity == identity)
                    .and_then(|info| cache.get(Path::new(&info.image_path)))
                    .map(|png| {
                        format!("data:image/png;base64,{}", STANDARD.encode(png.as_slice()))
                    });
                ProcessIcon { identity, data_url }
            })
            .collect())
    }
}
