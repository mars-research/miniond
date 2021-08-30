//! The `automount` applet.
//!
//! It mounts NFS shares configured in the experiment profile.

use async_trait::async_trait;
use serde::Deserialize;
use which::which;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::mount::Backend;
use super::{Applet, Sender, Message};

/// `autouser` applet configuration.
#[derive(Debug, Deserialize)]
pub struct AutomountConfig {
    /// Whether to enable the applet or not.
    enable: bool,

    /// The backend to use for mounting.
    backend: BackendConfig,
}

impl Default for AutomountConfig {
    fn default() -> Self {
        Self {
            enable: true,
            backend: BackendConfig::Systemd,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Deserialize)]
pub enum BackendConfig {
    /// Use systemd for mounting.
    #[serde(rename = "systemd")]
    Systemd,
}

/// The `autouser` applet.
#[derive(Debug)]
pub struct Automount {
    config: Config,
    tx: Sender,
}

impl Automount {
    pub(super) async fn new(config: Config, tx: Sender) -> Result<Box<dyn Applet>> {
        if config.automount.backend == BackendConfig::Systemd {
            if which("systemctl").is_err() {
                log::error!("The `systemctl` binary must be in PATH");
                return Err(Error::UnmetSystemRequirements);
            }
        }

        Ok(Box::new(Self {
            config,
            tx,
        }))
    }
}

#[async_trait]
impl Applet for Automount {
    async fn main(&self) -> Result<()> {
        let mut rx = self.tx.subscribe();

        if !self.config.automount.enable {
            log::info!("automount applet disabled in config");
            return Ok(());
        }

        let backend = match self.config.automount.backend {
            BackendConfig::Systemd => Backend::Systemd(self.config.systemd.unit_dir.clone()),
        };

        loop {
            let message = rx.recv().await.unwrap();
            match message {
                Message::Shutdown(_) => {
                    break;
                }

                Message::UpdateMounts(mounts) => {
                    log::info!("Got new mount configurations ({} mounts)", mounts.len());

                    for mount in mounts {
                        mount.apply(backend.clone()).await?;
                    }

                    self.tx.send(Message::UpdateMountsOk).unwrap();
                }

                _ => {}
            }
        }

        Ok(())
    }
}
