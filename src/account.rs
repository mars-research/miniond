//! Account management models.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::fs::{File, OpenOptions, create_dir_all};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use nix::unistd::{self, chown};
use users::{
    get_user_by_name,
    get_user_by_uid,
    get_group_by_name,
};

use crate::error::{Error, Result};

/// Type of a UID.
pub type Uid = u16;

/// Type of a GID.
pub type Gid = u16;

/// The fallback shell.
///
/// `/bin/sh` is the only shell that is mostly portable across
/// systems. Other shells may likely not exist.
const FALLBACK_SHELL: &str = "/bin/sh";

/// Path to the list of allowed shells.
const SHELLS_FILE: &str = "/etc/shells";

/// Account information returned by TMCD.
#[derive(Debug, Clone)]
pub struct Accounts {
    /// Users to be configured.
    pub users: HashMap<String, User>,

    /// Groups to be configured.
    pub groups: HashMap<String, Group>,
}

impl Accounts {
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            groups: HashMap::new(),
        }
    }
}

/// A user account.
#[derive(Debug, Clone)]
pub struct User {
    /// UNIX login.
    login: String,

    /// UID.
    uid: Uid,

    /// Primary group ID.
    gid: Gid,

    /// Whether the user has root access.
    root: bool,

    /// Home directory.
    home: PathBuf,

    /// SSH public keys.
    ssh_keys: Vec<String>,

    /// Login shell.
    shell: String,

    /// Opaque serial number.
    ///
    /// This indicates when the account information is changed.
    serial: String,
}

impl User {
    /// Create a new user account.
    ///
    /// This does not actually create the account in the system.
    pub fn new(login: String, uid: Uid, gid: Gid, serial: String) -> Self {
        let home = format!("/users/{}", &login).into();

        Self {
            login,
            uid,
            gid,
            root: false,
            home,
            ssh_keys: Vec::new(),
            shell: "bash".to_string(),
            serial,
        }
    }

    /// Add an SSH key.
    ///
    /// A `public_key` is a line in `authorized_keys`.
    pub fn add_ssh_key(&mut self, public_key: String) -> &mut Self {
        self.ssh_keys.push(public_key);
        self
    }

    /// Set whether the user has root privileges.
    pub fn root(&mut self, root: bool) -> &mut Self {
        self.root = root;
        self
    }

    /// Set the user's home.
    pub fn home(&mut self, home: PathBuf) -> &mut Self {
        self.home = home;
        self
    }

    /// Set the user's login shell.
    pub fn shell(&mut self, shell: String) -> &mut Self {
        self.shell = shell;
        self
    }

    /// Apply the configuration to the system.
    ///
    /// The user account will be created or modified as needed.
    /// User creation is complicated to get right, so we just run
    /// the `useradd` / `usermod` commands in the PATH.
    ///
    /// ## Resources
    ///
    /// The shadow-utils and FreeBSD implementations of `useradd` and `usermod`
    /// accept slightly different parameters. Here we use the common
    /// parameters supported by both implementations.
    ///
    /// - [shadow-utils useradd](https://www.mankier.com/8/useradd)
    /// - [FreeBSD
    /// useradd](https://www.freebsd.org/cgi/man.cgi?query=useradd&apropos=0&sektion=8&manpath=CentOS+6.0&arch=default&format=html)
    pub async fn apply(&self, system: &SystemConfiguration) -> Result<()> {
        let shell: &Path = match system.shells.get(&self.shell) {
            Some(path) => path,
            None => {
                log::warn!("{}'s preferred login shell \"{}\" is not installed. Using {} instead..."
                           , self.login, self.shell, FALLBACK_SHELL);

                Path::new(FALLBACK_SHELL)
            }
        };

        match get_user_by_name(&self.login) {
            Some(user) => {
                // Already exists
                let new_groups = user.groups()
                    .expect("User somehow disappeared")
                    .iter()
                    .map(|g| g.name().to_str().unwrap().to_string())
                    .filter(|gn| self.root || gn != &system.admin_group)
                    .collect::<Vec<String>>()
                    .join(",");

                if user.uid() != self.uid.into() {
                    return Err(Error::UidChangeUnsupported);
                }

                log::info!("Updating user {} with UID {}...", self.login, self.uid);

                let status = Command::new("usermod")
                    .arg("-s").arg(shell)
                    .args(&["-G", &new_groups])
                    .arg(&self.login)
                    .status().await?;

                if !status.success() {
                    return Err(Error::UserUpdate);
                }

                self.apply_authorized_keys().await?;

                Ok(())
            }
            None => {
                // New user
                if let Some(existing) = get_user_by_uid(self.uid.into()) {
                    return Err(Error::DuplicateUid {
                        login: self.login.clone(),
                        uid: self.uid,
                        existing_login: existing.name().to_string_lossy().to_string(),
                    });
                }

                let mut useradd = Command::new("useradd");

                useradd
                    .arg("--badname")
                    .arg("-md").arg(&self.home)
                    .args(&["-u", &self.uid.to_string()])
                    .args(&["-g", &self.gid.to_string()])
                    .arg("-s").arg(shell)
                    .arg("-N") // --no-user-group
                    .arg(&self.login);

                if self.root {
                    useradd.args(&["-G", &system.admin_group]);
                }

                log::info!("Creating user {} with UID {}...", self.login, self.uid);

                let status = useradd
                    .status().await?;

                if !status.success() {
                    return Err(Error::UserCreation);
                }

                self.apply_authorized_keys().await?;

                Ok(())
            }
        }
    }

    /// Apply the SSH public key configuration to the system.
    async fn apply_authorized_keys(&self) -> Result<()> {
        let authorized_keys = self.home.join(".ssh/authorized_keys");
        let ssh_dir = self.home.join(".ssh");

        create_dir_all(&ssh_dir).await?;

        log::info!("Updating SSH keys for user {}...", self.login);

        let mut file = OpenOptions::new()
            .read(false)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&authorized_keys)
            .await?;

        file.write_all("# This file was automatically generated by miniond\n".as_bytes()).await?;
        file.write_all("# Please add your keys using the testbed web interface.\n\n".as_bytes()).await?;

        for key in &self.ssh_keys {
            file.write_all(key.as_bytes()).await?;
            file.write_all("\n".as_bytes()).await?;
        }

        drop(file);

        {
            let uid = unistd::Uid::from_raw(self.uid.into());
            let gid = unistd::Gid::from_raw(self.gid.into());
            chown(&authorized_keys, Some(uid), Some(gid))?;
            chown(&ssh_dir, Some(uid), Some(gid))?;
        }

        Ok(())
    }
}

/// A group account.
#[derive(Debug, Clone)]
pub struct Group {
    /// Name.
    name: String,

    /// GID.
    gid: Gid,
}

impl Group {
    /// Create a new group.
    ///
    /// This does not actually create the group in the system.
    pub fn new(name: String, gid: Gid) -> Self {
        Self {
            name,
            gid,
        }
    }

    /// Apply the configuration to the system.
    ///
    /// We currently do not allow changes to a group.
    pub async fn apply(&self) -> Result<()> {
        match get_group_by_name(&self.name) {
            Some(group) => {
                // Existing group
                if group.gid() != self.gid.into() {
                    return Err(Error::GidChangeUnsupported);
                }

                Ok(())
            }
            None => {
                // New group
                log::info!("Creating group {} with GID {}", self.name, self.gid);

                let status = Command::new("groupadd")
                    .args(&["-g", &self.gid.to_string()])
                    .arg(&self.name)
                    .status().await?;

                if !status.success() {
                    return Err(Error::GroupCreation);
                }

                Ok(())
            }
        }
    }
}

/// System account configurations.
#[derive(Debug)]
pub struct SystemConfiguration {
    /// Cache of allowed login shells.
    ///
    /// Different systems place shell executables under different
    /// directories, and we cannot make assumptions of their full
    /// paths.
    shells: HashMap<String, PathBuf>,

    /// Group name for admins.
    ///
    /// Normally this would be "wheel" or "sudo".
    admin_group: String,
}

impl SystemConfiguration {
    pub async fn new(admin_group: Option<String>) -> Result<Self> {
        let file = File::open(SHELLS_FILE).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut shells = HashMap::new();

        while let Some(line) = lines.next_line().await? {
            let path = Path::new(&line);
            let name = path.file_name()
                .ok_or(Error::InvalidShellsFile)?
                .to_str()
                .ok_or(Error::InvalidShellsFile)?;

            if !shells.contains_key(name) {
                shells.insert(name.to_string(), path.to_path_buf());
            }
        }

        let admin_group = match admin_group {
            None => {
                let mut admin_group = "root".to_string();

                let group_candidates = vec![
                    "wheel",
                    "sudo",
                ];

                for group in group_candidates {
                    if get_group_by_name(group).is_some() {
                        admin_group = group.to_string();
                    }
                }

                admin_group
            }
            Some(g) => g,
        };

        Ok(Self {
            shells,
            admin_group,
        })
    }
}
