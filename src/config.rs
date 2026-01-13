use anyhow::{Context, Result};
use config::{Config, ConfigError, File};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub lines_from_bottom: usize,
}

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let proj_dirs = ProjectDirs::from("com", "rektide", "archive-list")
            .context("Failed to get project directories")?;

        let config_dir = proj_dirs.config_dir();
        std::fs::create_dir_all(config_dir)?;

        let config_path = config_dir.join("config.toml");

        Ok(Self { config_path })
    }

    pub fn load(&self) -> Result<AppConfig> {
        if !self.config_path.exists() {
            return Ok(AppConfig::default());
        }

        let config = Config::builder()
            .add_source(File::from(self.config_path.as_path()))
            .build()?;

        config.try_deserialize().map_err(|e: ConfigError| e.into())
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let toml = toml::to_string_pretty(config)?;
        std::fs::write(&self.config_path, toml)?;
        Ok(())
    }
}
