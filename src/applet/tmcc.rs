//! The `tmcc` applet.
//!
//! This applet uses `crate::tmcc` to communicate with the Testbed
//! Management Control Daemon (TMCD).

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde::Deserialize;

use crate::config::Config;
use crate::tmcc::{Tmcc as TmccClient, State, discover};
use crate::error::{Error, Result};
use super::{Applet, Sender, Message, ShutdownReason};

#[derive(Debug, Deserialize)]
pub struct TmccConfig {
    /// The boss node.
    ///
    /// By default this will be automatically discovered.
    boss: Option<String>,
}

impl Default for TmccConfig {
    fn default() -> Self {
        Self {
            boss: None,
        }
    }
}

/// The `tmcc` applet.
pub struct Tmcc {
    tmcc: TmccClient,
    tx: Sender,
    account_initialized: AtomicBool,
}

impl Tmcc {
    pub(super) async fn new(config: Config, tx: Sender) -> Result<Box<dyn Applet>> {
        log::info!("Looking for the boss node...");
        let boss = if let Some(boss) = &config.tmcc.boss {
            boss.clone()
        } else {
            discover().await?
        };

        let tmcc = TmccClient::new(boss)?;

        Ok(Box::new(Self {
            tmcc,
            tx,
            account_initialized: AtomicBool::new(false),
        }))
    }
}

#[async_trait]
impl Applet for Tmcc {
    async fn main(&self) -> Result<()> {
        let mut rx = self.tx.subscribe();

        log::info!("Informing testbed that we have booted...");
        self.tmcc.state(&State::Setup).await?;

        self.tx.send(Message::ReloadTestbed).unwrap();

        loop {
            let message = rx.recv().await.unwrap();

            match message {
                Message::Shutdown(reason) => {
                    if reason == ShutdownReason::Signal {
                        log::info!("Informing testbed that we are shutting down...");
                        self.tmcc.state(&State::Shutdown).await.unwrap();
                    }
                    break;
                }
                Message::UpdateAccountsOk => {
                    if !self.account_initialized.load(Ordering::Relaxed) {
                        log::info!("Informing testbed that we are ready...");
                        self.tmcc.state(&State::Up).await?;
                        self.account_initialized.store(true, Ordering::Relaxed);
                    }
                }
                Message::ReloadTestbed => {
                    log::info!("Reloading information from testbed...");

                    let (accounts, mounts, hostinfo) = tokio::join!(
                        async {
                            let accounts = self.tmcc.accounts().await?;
                            self.tx.send(Message::UpdateAccounts(accounts)).unwrap();

                            Result::Ok(())
                        },
                        async {
                            let mounts = self.tmcc.mounts().await?;
                            self.tx.send(Message::UpdateMounts(mounts)).unwrap();

                            Result::Ok(())
                        },
                        async {
                            match self.tmcc.allocation_status().await? {
                                Some(allocation) => {
                                    let manifest = self.tmcc.geni_manifest().await?;
                                    let current_node = manifest.get_node(&allocation.node_name)
                                        .ok_or(Error::GeniNoSuchNode)?;

                                    let fqdn = current_node.fqdn();
                                    let ipv4 = current_node.ipv4();

                                    log::info!("Our FQDN: {} -> {}", fqdn, ipv4);

                                    self.tx.send(Message::UpdateCanonical(fqdn, ipv4)).unwrap();
                                }
                                None => {
                                    log::warn!("The current node is (no longer) allocated!");
                                }
                            }

                            Result::Ok(())
                        },
                    );

                    accounts?; mounts?; hostinfo?;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
