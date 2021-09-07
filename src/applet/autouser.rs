//! The `autouser` applet.
//!
//! It creates and configures users and groups.

use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use which::which;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::account::SystemConfiguration;
use super::{Applet, Sender, Message};

/// `autouser` applet configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct AutouserConfig {
    /// Whether to enable the applet or not.
    enable: bool,

    /// Name of the admin group.
    ///
    /// If unset, one will be automatically discovered (`wheel`, `sudo`, `root`).
    #[serde(rename = "admin-group")]
    admin_group: Option<String>,
}

impl Default for AutouserConfig {
    fn default() -> Self {
        Self {
            enable: true,
            admin_group: None,
        }
    }
}

/// The `autouser` applet.
#[derive(Debug)]
pub struct Autouser {
    config: Config,
    system: SystemConfiguration,
    tx: Sender,
}

impl Autouser {
    pub(super) async fn new(config: Config, tx: Sender) -> Result<Box<dyn Applet>> {
        if !check_requirements() {
            return Err(Error::UnmetSystemRequirements);
        }

        let admin_group = config.autouser.admin_group.clone();
        let system = SystemConfiguration::new(admin_group).await?;

        Ok(Box::new(Self {
            config,
            system,
            tx,
        }))
    }
}

#[async_trait]
impl Applet for Autouser {
    async fn main(&self) -> Result<()> {
        let mut rx = self.tx.subscribe();

        if !self.config.autouser.enable {
            log::info!("autouser applet disabled in config");
            return Ok(());
        }

        loop {
            let message = rx.recv().await.unwrap();
            match message {
                Message::Shutdown(_) => {
                    break;
                }

                Message::UpdateAccounts(accounts) => {
                    log::info!("Got new account configurations (Users: {}, Groups: {})", accounts.users.len(), accounts.groups.len());

                    {
                        let mut futures = Vec::new();

                        for group in accounts.groups.values() {
                            futures.push(group.apply());
                        }

                        for res in join_all(futures).await {
                            res?;
                        }
                    }

                    {
                        let mut futures = Vec::new();

                        for user in accounts.users.values() {
                            futures.push(user.apply(&self.system));
                        }

                        for res in join_all(futures).await {
                            res?;
                        }
                    }

                    log::info!("Successfully applied account configurations");

                    self.tx.send(Message::UpdateAccountsOk).unwrap();
                }

                _ => {}
            }
        }

        Ok(())
    }
}

fn check_requirements() -> bool {
    let commands = vec![
        "useradd",
        "groupadd",
        "usermod",
        "groupmod",
    ];

    let mut check = true;

    for command in commands {
        if which(command).is_err() {
            log::error!("The `{}` binary must be in PATH", command);
            check = false;
        }
    }

    check
}
