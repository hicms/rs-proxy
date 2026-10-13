use crate::config::{Config, ConfigError};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Invalid(#[from] ConfigError),
    #[error("配置文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("配置 JSON 无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("配置已被更新，请刷新后再保存")]
    Conflict,
}

pub struct ConfigStore {
    path: PathBuf,
    current: Config,
}

impl ConfigStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let config = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Config>(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(error) => return Err(error.into()),
        };
        config.validate()?;
        Ok(Self {
            path,
            current: config,
        })
    }

    pub fn config(&self) -> &Config {
        &self.current
    }

    pub fn replace(&mut self, mut config: Config) -> Result<Config, StoreError> {
        config.validate()?;
        if config.revision != self.current.revision {
            return Err(StoreError::Conflict);
        }
        config.revision = config.revision.checked_add(1).ok_or(StoreError::Conflict)?;
        atomic_write(&self.path, &serde_json::to_vec_pretty(&config)?)?;
        self.current = config;
        Ok(self.current.clone())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
