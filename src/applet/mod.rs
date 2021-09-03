//! Applet framework.
//!
//! As a single-binary daemon, miniond implements distinct
//! features as applets. Applets run concurrently and communicate
//! with each other via a Tokio broadcast channel (think of it
//! as a shared bus), sending typed Rust values.
//!
//! ```
//! [tmcc] -- [autouser]
//!     \---- [automount]
//! ```
//!
//! In the above example, the `tmcc` applet may request account
//! information from the testbed and then send a `Message::UpdateAccount`
//! message through the channel.

mod autouser;
mod automount;
mod autohost;
mod tmcc;
mod signal;

// use std::future::Future;
use std::net::Ipv4Addr;

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::mount::NfsMount;
use crate::account::Accounts;
use crate::config::Config;
use crate::error::Result;

pub use autouser::{Autouser, AutouserConfig};
pub use automount::{Automount, AutomountConfig};
pub use autohost::{Autohost, AutohostConfig};
pub use tmcc::{Tmcc, TmccConfig};
pub use signal::Signal;

const CHANNEL_CAPACITY: usize = 100;

type Sender = broadcast::Sender<Message>;

/// A message.
#[derive(Debug, Clone)]
enum Message {
    /// Shut down the daemon.
    Shutdown(ShutdownReason),

    /// Update accounts on the system.
    UpdateAccounts(Accounts),

    /// Account update was successful.
    UpdateAccountsOk,

    /// Update NFS mounts on the system.
    UpdateMounts(Vec<NfsMount>),

    /// Mount update was successful.
    UpdateMountsOk,

    /// Update FQDN and its associated IP of the system.
    UpdateCanonical(String, Ipv4Addr),

    /// Reload information from the testbed.
    ReloadTestbed,
}

/// A shutdown reason.
///
/// Depending on the reason, we may or may not inform
/// the testbed before we exit.
///
/// This reason also determines the exit code.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ShutdownReason {
    /// Received a non-interactive terminating signal.
    ///
    /// We will report to the testbed that we are shutting down.
    Signal,

    /// Received an interactive terminating signal (e.g., Ctrl-C).
    InteractiveSignal,
}

/// An applet.
#[async_trait]
trait Applet {
    /// Entry point of the applet.
    async fn main(&self) -> Result<()>;
}

/// Run a single applet with automatic restart.
async fn run_applet(name: &'static str, applet: Box<dyn Applet>) {
    loop {
        match applet.main().await {
            Ok(()) => {
                log::debug!("Applet {} exited.", name);
                break;
            }
            Err(e) => {
                log::error!("Applet {} exited with error: {}", name, e);
                log::warn!("Trying to respawn...");
            }
        }
    }
}

/// Run all applets.
pub async fn run(config: Config) -> Result<()> {
    let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
    drop(rx);

    let signal = Signal::new(tx.clone());
    let autouser = Autouser::new(config.clone(), tx.clone()).await?;
    let automount = Automount::new(config.clone(), tx.clone()).await?;
    let autohost = Autohost::new(config.clone(), tx.clone()).await?;
    let tmcc = Tmcc::new(config.clone(), tx.clone()).await?;

    log::info!("Starting all applets...");

    tokio::join!(
        run_applet("signal", signal),

        run_applet("tmcc", tmcc),
        run_applet("autouser", autouser),
        run_applet("automount", automount),
        run_applet("autohost", autohost),
    );

    Ok(())
}
