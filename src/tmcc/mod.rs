//! The Testbed Master Control Client (tmcc).
//!
//! The corresponding applet is `applet/tmcc.rs`.
//!
//! ## Resources
//!
//! - <https://wiki.emulab.net/wiki/TmcdApi>

mod discovery;
mod parser;

use std::convert::AsRef;
use std::net::SocketAddr;

use tokio::net::{
    TcpStream,
    lookup_host,
};
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

/// A boss node.
pub enum BossNode {
    /// A host-port tuple.
    HostPort((String, u16)),

    /*
    /// A well-typed SocketAddr.
    SocketAddr(SocketAddr),
    */
}

impl BossNode {
    fn host(host: String) -> Self {
        Self::HostPort((host, TMCD_PORT))
    }

    async fn to_socket_addr(self) -> Result<SocketAddr> {
        match self {
            Self::HostPort(host_port) => {
                if let Some(sa) = lookup_host(host_port.clone()).await?.next() {
                    Ok(sa)
                } else {
                    Err(Error::EmulabBossUnresolvable { host_port })
                }
            }
            /*
            Self::SocketAddr(sa) => Ok(sa),
            */
        }
    }
}

/// A TMCD client.
pub struct Tmcc {
    boss: SocketAddr,
}

impl Tmcc {
    /// Create a new testbed master control client with a specific boss node.
    pub async fn new(boss: BossNode) -> Result<Self> {
        let sa = boss.to_socket_addr().await?;

        Ok(Self {
            boss: sa,
        })
    }

    /// Automatically discover the boss node.
    pub async fn discover() -> Result<Self> {
        let boss = discovery::discover().await?;

        Self::new(boss).await
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

        // also get root account info
        let root = self.root_account().await?;
        accounts.users.insert("root".to_string(), root);

        Ok(accounts)
    }

    /// Retrieve root account information.
    async fn root_account(&self) -> Result<User> {
        use users::os::unix::UserExt;

        let mut socket = self.connect().await?;

        Command::new("localization")
            .send(&mut socket).await?;

        let root_sys = users::get_user_by_uid(0)
            .ok_or(Error::TmcdNoSuchUser { login: "root".to_string() })?;

        let mut root = User::new(
            "root".to_string(),
            0, 0,
            "".to_string(),
        );
        root.home(root_sys.home_dir().to_path_buf());

        let mut line = String::new();
        loop {
            let len = socket.read_line(&mut line).await?;

            if len == 0 {
                break;
            }

            // We currently do not handle multi-line responses, so
            // this is expected to fail for the ROOTKEY lines.
            match Response::parse(line.trim()) {
                Ok(r) => {
                    if let Ok(pubkey) = r.get_parsed("ROOTPUBKEY") {
                        root.add_ssh_key(pubkey);
                    } else {
                        log::debug!("Encountered first line without public key - skipping the rest");
                        break;
                    }
                }
                Err(e) => {
                    log::debug!("Silently ignoring LOCALIZATION parse error: {:?}", e);
                    break;
                }
            }

            line.clear();
        }

        Ok(root)
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
        let stream = TcpStream::connect(self.boss).await?;
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
