//! The Testbed Master Control Client (tmcc).
//!
//! The corresponding applet is `applet/tmcc.rs`.
//!
//! ## Resources
//!
//! - <https://wiki.emulab.net/wiki/TmcdApi>

mod parser;

use std::convert::AsRef;
use std::env;

use resolv_conf::{Config as ResolvConf, ScopedIp};
use tokio::fs::read_to_string;
use tokio::net::TcpStream;
use tokio::io::{
    BufStream,
    AsyncBufReadExt,
    AsyncWriteExt,
};

use crate::account::{Accounts, User, Group};
use crate::error::{Error, Result};
use crate::geni::RSpec;
use crate::mount::NfsMount;
use parser::Response;

/// The default TMCD port.
pub const TMCD_PORT: u16 = 7777;

/// The TMCD protocol version.
///
/// This number is from
/// <https://gitlab.flux.utah.edu/emulab/emulab-devel/-/blob/223096154f87ac7708a0f87a1bb63a20ef0fbde7/clientside/lib/tmcd/tmcd.h#L49>.
pub const TMCD_VERSION: usize = 44;

/// A TMCD client.
pub struct Tmcc {
    host: String,
    port: u16,
}

impl Tmcc {
    pub fn new(host: String) -> Result<Self> {
        if host.find(':').is_some() {
            let parts: Vec<&str> = host.split(':').collect();
            if parts.len() != 2 {
                return Err(Error::TmcdBadBossNode { host });
            }

            let port: u16 = parts[1].parse()
                .or(Err(Error::TmcdBadBossNode { host: host.clone() }))?;

            Ok(Self {
                host: parts[0].to_string(),
                port,
            })
        } else {
            Ok(Self {
                host,
                port: TMCD_PORT,
            })
        }
    }

    /// Retrieve accounts that should be configured.
    pub async fn accounts(&self) -> Result<Accounts> {
        let mut socket = self.connect().await?;

        Command::new("accounts")
            .send(&mut socket).await?;

        let mut accounts = Accounts::new();

        let mut line = String::new();
        loop {
            let len = socket.read_line(&mut line).await?;

            if len == 0 {
                break;
            }

            let parsed = Response::parse(line.trim())?;
            match parsed.response_type() {
                Some("ADDUSER") => {
                    let login: String = parsed.get_parsed("LOGIN")?;

                    let mut user = User::new(
                        login.clone(),
                        parsed.get_parsed("UID")?,
                        parsed.get_parsed("GID")?,
                        parsed.get_parsed("SERIAL")?,
                    );

                    user
                        .root(parsed.get("ROOT")? == &"1")
                        .home(parsed.get_parsed("HOMEDIR")?)
                        .shell(parsed.get_parsed("SHELL")?);

                    if accounts.users.insert(login.clone(), user).is_some() {
                        return Err(Error::TmcdDuplicateUser {
                            login,
                        });
                    }
                }
                Some("PUBKEY") => {
                    let login: String = parsed.get_parsed("LOGIN")?;
                    let key: String = parsed.get_parsed("KEY")?;

                    if let Some(user) = accounts.users.get_mut(&login) {
                        user.add_ssh_key(key);
                    } else {
                        return Err(Error::TmcdNoSuchUser {
                            login,
                        });
                    }
                }
                Some("ADDGROUP") => {
                    let mut name: String = parsed.get_parsed("NAME")?;

                    // Here we convert the group name to lowercase for
                    // compatibility. The shadow-utils implementation of
                    // groupadd does not allow group names to contain
                    // upper-case letters.
                    name.make_ascii_lowercase();

                    let group = Group::new(
                        name.clone(),
                        parsed.get_parsed("GID")?,
                    );

                    if accounts.groups.insert(name.clone(), group).is_some() {
                        return Err(Error::TmcdDuplicateGroup {
                            name,
                        });
                    }
                }
                Some("SFSKEY") => {
                    log::warn!("Received unsupported SFSKEY directive");
                }
                Some(directive) => {
                    return Err(Error::TmcdUnknownDirective {
                        directive: directive.to_string(),
                        line: line.to_string(),
                    });
                }
                None => {
                    return Err(Error::TmcdMissingDirective {
                        line: line.to_string(),
                    });
                }
            }

            line.clear();
        }

        Ok(accounts)
    }

    /// Retrieve mounts that should be configured.
    pub async fn mounts(&self) -> Result<Vec<NfsMount>> {
        let mut socket = self.connect().await?;
        let mut mounts = Vec::new();

        Command::new("mounts")
            .send(&mut socket).await?;

        let mut line = String::new();
        loop {
            let len = socket.read_line(&mut line).await?;

            if len == 0 {
                break;
            }

            let parsed = Response::parse(line.trim())?;
            if let Ok(remote) = parsed.get_parsed::<String>("REMOTE") {
                let local = parsed.get_parsed("LOCAL")?;

                mounts.push(NfsMount::new(remote, local));
            } else {
                log::debug!("Non mountpoint line: {}", line);
            }

            line.clear();
        }

        Ok(mounts)
    }

    /// Inform the testbed of our new state.
    pub async fn state(&self, state: &State) -> Result<()> {
        let mut socket = self.connect().await?;

        Command::new("state")
            .arg(state.as_ref())
            .send(&mut socket).await?;

        Ok(())
    }

    /// Retrieve the allocation status for the current node.
    pub async fn allocation_status(&self) -> Result<Option<AllocationStatus>> {
        let mut socket = self.connect().await?;

        Command::new("status")
            .send(&mut socket).await?;

        let mut line = String::new();
        socket.read_line(&mut line).await?;

        let parsed = Response::parse(line.trim())?;

        if let Some("FREE") = parsed.response_type() {
            // Not allocated
            Ok(None)
        } else {
            // Allocated
            let status = AllocationStatus {
                experiment: parsed.get_parsed("ALLOCATED")?,
                node_name: parsed.get_parsed("NICKNAME")?,
            };

            Ok(Some(status))
        }
    }

    /// Retrieve the GENI manifest.
    ///
    /// Adapted from the `/usr/bin/geni-get` script.
    pub async fn geni_manifest(&self) -> Result<RSpec> {
        let mut socket = self.connect().await?;

        socket.write_all("geni_manifest".as_bytes()).await?;
        socket.flush().await?;

        let mut buf = Vec::new();
        let first_byte_len = socket.read_until(0, &mut buf).await?;

        let response = if first_byte_len > 1 {
            // Dump everything
            &buf[..]
        } else if first_byte_len == 1 {
            buf.clear();
            let rest_len = socket.read_until(0, &mut buf).await?;

            if rest_len == 1 {
                return Err(Error::TmcdGeniError);
            }

            &buf[..]
        } else {
            return Err(Error::TmcdGeniBlankResponse);
        };

        let xml = std::str::from_utf8(response)
            .or(Err(Error::TmcdInvalidUtf8))?;

        let rspec: RSpec = serde_xml_rs::from_str(&xml)
            .map_err(|error| Error::GeniParseError { error })?;

        Ok(rspec)
    }

    async fn connect(&self) -> Result<BufStream<TcpStream>> {
        let stream = TcpStream::connect((self.host.as_str(), self.port)).await?;
        Ok(BufStream::new(stream))
    }
}

/// The node allocation status.
pub struct AllocationStatus {
    pub experiment: String,
    pub node_name: String,
}

/// Current state of the system.
#[derive(Debug)]
pub enum State {
    /// The system is up.
    Up,

    /// The system is being set up.
    Setup,

    /// The system is (being) shut down.
    Shutdown,
}

impl AsRef<str> for State {
    fn as_ref(&self) -> &str {
        match self {
            Self::Up => "ISUP",
            Self::Setup => "MFSSETUP",
            Self::Shutdown => "SHUTDOWN",
        }
    }
}

/// A TMCD command.
struct Command {
    bytes: Vec<u8>,
}

impl Command {
    /// Create a new command.
    pub fn new(command: &str) -> Self {
        let mut bytes = format!("VERSION={} ", TMCD_VERSION).into_bytes();
        bytes.extend_from_slice(command.as_bytes());
        Self {
            bytes,
        }
    }

    /// Add an argument.
    pub fn arg(mut self, arg: &str) -> Self {
        self.bytes.push(' ' as u8);
        self.bytes.extend_from_slice(arg.as_bytes());
        self
    }

    /// Finalize the command and send it to a socket.
    pub async fn send(self, stream: &mut BufStream<TcpStream>) -> Result<()> {
        stream.write_all(&self.finalize()).await?;
        stream.flush().await?;

        Ok(())
    }

    /// Finalize the command, returning the bytes to be sent.
    pub fn finalize(mut self) -> Vec<u8> {
        self.bytes.push(' ' as u8);
        self.bytes
    }
}

/// Discover the boss node automatically.
pub async fn discover() -> Result<String> {
    if let Ok(boss) = env::var("BOSSNODE") {
        log::info!("Discovered boss node from BOSSNODE environment variable: {}", boss);
        return Ok(boss);
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
                return Ok(boss.to_string());
            }
            Err(_) => {}
        }
    }

    if let Some(boss) = discover_from_resolv_conf().await {
        log::info!("Discovered boss node from /etc/resolv.conf: {}", boss);
        return Ok(boss);
    }

    Err(Error::TmcdFailedToDiscoverBossNode)
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
