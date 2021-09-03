//! Error types.

use std::io;

use snafu::Snafu;

use crate::account::Uid;

pub type Result<T> = std::result::Result<T, Error>;

/// An error.
///
/// A bit too pedantic for my taste.
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Malformed TMCD boss node specification: {}", host))]
    TmcdBadBossNode { host: String },

    #[snafu(display("Failed to discover TMCD boss node"))]
    TmcdFailedToDiscoverBossNode,

    #[snafu(display("Got Non-UTF8 TMCD response"))]
    TmcdInvalidUtf8,

    #[snafu(display("Bad TMCD response (position {}): {}", position, line))]
    TmcdBadLine { line: String, position: usize },

    #[snafu(display("Required key {} missing from TMCD response: {}", key, line))]
    TmcdMissingKey { key: String, line: String },

    #[snafu(display("Duplicate user {} in TMCD response", login))]
    TmcdDuplicateUser { login: String },

    #[snafu(display("Duplicate group {} in TMCD response", name))]
    TmcdDuplicateGroup { name: String },

    #[snafu(display("Missing directive in TMCD response: {}", line))]
    TmcdMissingDirective { line: String },

    #[snafu(display("Unknown directive {} in TMCD response: {}", directive, line))]
    TmcdUnknownDirective { directive: String, line: String },

    #[snafu(display("Invalid value {} from TMCD response: {}", value, parse_error))]
    TmcdBadValue { value: String, parse_error: Box<dyn std::error::Error + Send + Sync> },

    #[snafu(display("Invalid user {} from TMCD response", login))]
    TmcdNoSuchUser { login: String },

    #[snafu(display("TMCD returned blank GENI response"))]
    TmcdGeniBlankResponse,

    #[snafu(display("TMCD returned unknown GENI error"))]
    TmcdGeniError,

    #[snafu(display("GENI parsing error: {}", error))]
    GeniParseError { error: serde_xml_rs::Error },

    #[snafu(display("The current node does not exist in the GENI manifest (has the reservastion expired?)"))]
    GeniNoSuchNode,

    #[snafu(display("Attempted to create user {} with non-unique UID {} (already used by {})", login, uid, existing_login))]
    DuplicateUid { login: String, uid: Uid, existing_login: String },

    #[snafu(display("Invalid /etc/shells file"))]
    InvalidShellsFile,

    #[snafu(display("Failed to create user account."))]
    UserCreation,

    #[snafu(display("Failed to create group account."))]
    GroupCreation,

    #[snafu(display("Failed to update user account."))]
    UserUpdate,

    #[snafu(display("Failed to mount."))]
    Mount,

    #[snafu(display("Changing UIDs is not supported"))]
    UidChangeUnsupported,

    #[snafu(display("Changing GIDs is not supported"))]
    GidChangeUnsupported,

    #[snafu(display("Unmet system requirements"))]
    UnmetSystemRequirements,

    /// The SRV record indicates that a boss node is definitely not available.
    ///
    /// This is returned when a SRV lookup returns "." as the result.
    ///
    /// - <https://datatracker.ietf.org/doc/html/rfc2782>
    #[snafu(display("SRV record indicates the absence of a boss node"))]
    EmulabBossSrvNotAvailable,

    #[snafu(display("The supplied boss node cannot be resolved: {:?}", host_port))]
    EmulabBossUnresolvable { host_port: (String, u16) },

    #[snafu(display("I/O error: {}", error))]
    IoError { error: io::Error },

    #[snafu(display("OS error: {}", error))]
    NixError { error: nix::errno::Errno },

    #[snafu(display("DNS lookup error: {}", error))]
    DnsLookupError { error: trust_dns_resolver::error::ResolveError },
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::IoError { error }
    }
}

impl From<nix::errno::Errno> for Error {
    fn from(error: nix::errno::Errno) -> Self {
        Self::NixError { error }
    }
}

impl From<trust_dns_resolver::error::ResolveError> for Error {
    fn from(error: trust_dns_resolver::error::ResolveError) -> Self {
        Self::DnsLookupError { error }
    }
}
