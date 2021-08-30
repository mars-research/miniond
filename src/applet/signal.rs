//! Signal handler applet.

use async_trait::async_trait;
use tokio::signal::unix::{SignalKind, signal};

use crate::error::Result;
use super::{Applet, Sender, Message, ShutdownReason};

async fn watch(kind: SignalKind, message: Message, tx: Sender) {
    signal(kind).unwrap().recv().await;

    log::info!("Received signal {:?}. Broadcasting {:?} to applets...", kind, message);
    tx.send(message).unwrap();
}

pub struct Signal {
    tx: Sender,
}

impl Signal {
    pub(super) fn new(tx: Sender) -> Box<dyn Applet> {
        Box::new(Self { tx })
    }
}

#[async_trait]
impl Applet for Signal {
    async fn main(&self) -> Result<()> {
        tokio::select!(
            _ = watch(
                SignalKind::terminate(),
                Message::Shutdown(ShutdownReason::Signal),
                self.tx.clone()
            ) => (),

            _ = watch(
                SignalKind::interrupt(),
                Message::Shutdown(ShutdownReason::InteractiveSignal),
                self.tx.clone()
            ) => (),

            _ = watch(
                SignalKind::hangup(),
                Message::ReloadTestbed,
                self.tx.clone()
            ) => (),
        );

        Ok(())
    }
}
