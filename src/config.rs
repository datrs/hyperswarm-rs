use std::net::{SocketAddr, ToSocketAddrs};

use crate::discovery::dht::DhtConfig;
use crate::discovery::mdns::MdnsConfig;

#[derive(Debug, Default)]
pub struct Config {
    pub mdns: Option<MdnsConfig>,
    pub dht: Option<DhtConfig>,
    pub local_addr: Option<SocketAddr>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            mdns: Some(MdnsConfig::default()),
            dht: Some(DhtConfig::default()),
            local_addr: None,
        }
    }

    pub fn with_defaults(mut self) -> Self {
        self.mdns = self.mdns.or_else(Default::default);
        self.dht = self.dht.or_else(Default::default);
        self
    }

    /// Set DHT config
    pub fn set_dht(mut self, config: DhtConfig) -> Self {
        self.dht = Some(config);
        self
    }

    /// Set MDNS config
    pub fn set_mdns(mut self, config: MdnsConfig) -> Self {
        self.mdns = Some(config);
        self
    }

    /// Set local address bind TCP and UTP sockets for incoming connections on
    pub fn set_local_addr(mut self, address: SocketAddr) -> Self {
        self.local_addr = Some(address);
        self
    }

    // shortcuts, not sure if this is good style?

    pub fn set_bootstrap_nodes<T: ToSocketAddrs>(mut self, addresses: &[T]) -> Self {
        self.dht = Some(self.dht.unwrap_or_default().set_bootstrap_nodes(addresses));
        self
    }

    pub fn set_ephemeral(mut self, ephemeral: bool) -> Self {
        self.dht = Some(self.dht.unwrap_or_default().set_ephemeral(ephemeral));
        self
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct TopicConfig {
    pub announce: bool,
    pub lookup: bool,
}

impl TopicConfig {
    pub fn both() -> Self {
        Self {
            announce: true,
            lookup: true,
        }
    }

    pub fn announce_and_lookup() -> Self {
        Self::both()
    }
}
