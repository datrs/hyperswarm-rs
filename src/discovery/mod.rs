use async_std::stream::Stream;
use std::fmt;
use std::io;
use std::net::SocketAddr;

pub mod combined;
pub mod dht;
pub mod mdns;

pub type Topic = [u8; 32];

#[derive(Clone, Debug)]
pub enum DiscoveryMethod {
    Mdns,
    Dht,
}

#[derive(Clone)]
pub struct PeerInfo {
    addr: SocketAddr,
    topic: Option<Topic>,
    discovery_method: DiscoveryMethod,
}

impl fmt::Debug for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerInfo")
            .field("addr", &self.addr)
            .field(
                "topic",
                &self.topic.map(|topic| pretty_hash::fmt(&topic).unwrap()),
            )
            .field("discovery_method", &self.discovery_method)
            .finish()
    }
}

impl PeerInfo {
    pub fn new(addr: SocketAddr, topic: Option<Topic>, discovery_method: DiscoveryMethod) -> Self {
        Self {
            addr,
            topic,
            discovery_method,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

pub trait Discovery: Stream<Item = io::Result<PeerInfo>> {
    fn lookup(&mut self, topic: Topic);
    fn announce(&mut self, topic: Topic);
}
