//! Configuration.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

use crate::applet::{
    AutouserConfig,
    AutomountConfig,
    AutohostConfig,
    TmccConfig,
};

pub type Config = Arc<ConfigInner>;

#[derive(Debug, Default, Deserialize)]
pub struct ConfigInner {
    /// `autouser` applet configuration.
    #[serde(default)]
    pub autouser: AutouserConfig,

    /// `automount` applet configuration.
    #[serde(default)]
    pub automount: AutomountConfig,

    /// `autohost` applet configuration.
    #[serde(default)]
    pub autohost: AutohostConfig,

    /// `tmcc` applet configuration.
    #[serde(default)]
    pub tmcc: TmccConfig,

    /// Systemd integration configuration.
    #[serde(default)]
    pub systemd: SystemdConfig,
}

#[derive(Debug, Deserialize)]
pub struct SystemdConfig {
    /// Path to the systemd unit directory
    #[serde(rename = "unit-dir")]
    pub unit_dir: PathBuf,
}

impl Default for SystemdConfig {
    fn default() -> Self {
        Self {
            unit_dir: PathBuf::from("/etc/systemd/system"),
        }
    }
}

pub fn get_config(path: Option<PathBuf>) -> Config {
    let inner = match path {
        None => {
            ConfigInner::default()
        }
        Some(path) => {
            let config = fs::read_to_string(path)
                .expect("Failed to read config file");
            toml::from_str(&config)
                .expect("Failed to parse config file")
        }
    };

    Arc::new(inner)
}
