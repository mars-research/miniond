//! GENI models.
//!
//! We just do the bare mininum that's enough to get
//! the full FQDN.

use std::net::Ipv4Addr;

use serde::Deserialize;

/// GENI Resource Specification.
///
/// <https://groups.geni.net/geni/wiki/GENIExperimenter/RSpecs>.
#[derive(Debug, Deserialize)]
pub struct RSpec {
    #[serde(rename = "node", default)]
    nodes: Vec<Node>,
}

impl RSpec {
    pub fn get_node(&self, client_id: &str) -> Option<&Node> {
        self.nodes.iter().find(|e| e.client_id == client_id)
    }
}

#[derive(Debug, Deserialize)]
pub struct Node {
    client_id: String,
    host: Host,
}

impl Node {
    /// Returns the FQDN of the node.
    pub fn fqdn(&self) -> String {
        self.host.name.clone()
    }

    /// Returns the IPv4 address of the node.
    pub fn ipv4(&self) -> Ipv4Addr {
        self.host.ipv4.clone()
    }
}

#[derive(Debug, Deserialize)]
struct Host {
    name: String,
    ipv4: Ipv4Addr,
}
