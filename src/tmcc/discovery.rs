//! Boss node discovery.

use std::env;

use tokio::fs::read_to_string;
use resolv_conf::{Config as ResolvConf, ScopedIp};
use trust_dns_resolver::AsyncResolver;
use trust_dns_resolver::error::ResolveErrorKind;

use crate::error::{Error, Result};
use super::BossNode;

/// Name of the SRV record that contains the boss node address.
const EMULAB_BOSS_SRV: &'static str = "_emulab_boss";

/// Discover the boss node automatically.
pub async fn discover() -> Result<BossNode> {
    if let Ok(boss) = env::var("BOSSNODE") {
        log::info!("Discovered boss node from BOSSNODE environment variable: {}", boss);
        return Ok(BossNode::host(boss));
    }

    let files = vec![
        "/etc/testbed",
        "/etc/emulab",
        "/etc/rc.d/testbed",
        "/usr/local/etc/testbed",
        "/usr/local/etc/emulab",
    ];

    for file in files {
        match read_to_string(&file).await {
            Ok(boss) => {
                let boss = boss.trim();

                log::info!("Discovered boss node from {}: {}", file, boss);
                return Ok(BossNode::host(boss.to_string()));
            }
            Err(_) => {}
        }
    }

    if let Ok(host_port) = discover_from_srv_record().await {
        log::info!("Discovered boss node from SRV record: {:?}", host_port);
        return Ok(BossNode::HostPort(host_port));
    }

    if let Some(boss) = discover_from_resolv_conf().await {
        log::info!("Discovered boss node from /etc/resolv.conf: {}", boss);
        return Ok(BossNode::host(boss));
    }

    Err(Error::TmcdFailedToDiscoverBossNode)
}

/// Discover the boss node from SRV record.
///
/// The boss node may be discoverable through the `_emulab_boss`
/// SRV record in the search domain.
///
/// This was added in the Wisconsin cluster as a test in:
/// <https://groups.google.com/g/cloudlab-users/c/6fRdB7ykOFQ/m/1_HvTebRBgAJ>
async fn discover_from_srv_record() -> Result<(String, u16)> {
    let resolver = AsyncResolver::tokio_from_system_conf()?;
    match resolver.srv_lookup(EMULAB_BOSS_SRV).await {
        Ok(records) => {
            let first = records.iter().next().expect("No record is available");

            if first.target().is_root() {
                Err(Error::EmulabBossSrvNotAvailable)
            } else {
                Ok((first.target().to_ascii(), first.port()))
            }
        }
        Err(e) => {
            if let ResolveErrorKind::NoRecordsFound { .. } = e.kind() {
                log::debug!("SRV lookup returned no results: {:?}", e);
            } else {
                log::warn!("SRV lookup returned error: {:?}", e);
            }
            Err(e.into())
        }
    }
}

async fn discover_from_resolv_conf() -> Option<String> {
    let conf = read_to_string("/etc/resolv.conf").await.map_err(|e| {
        log::warn!("Error trying to read /etc/resolv.conf: {}", e);
        e
    }).ok()?;

    let parsed = ResolvConf::parse(&conf).map_err(|e| {
        log::warn!("Error trying to parse /etc/resolv.conf: {}", e);
        e
    }).ok()?;

    if parsed.nameservers.is_empty() {
        return None;
    }

    let first_dns = &parsed.nameservers[0];
    match first_dns {
        ScopedIp::V4(addr) => {
            if addr.is_link_local() || addr.is_loopback() {
                None
            } else {
                Some(addr.to_string())
            }
        }
        ScopedIp::V6(addr, _) => {
            Some(addr.to_string())
        }
    }
}
