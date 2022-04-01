use async_std::stream::StreamExt;
use async_std::task::JoinHandle;
use hyperswarm_dht::{DhtConfig, HyperDht};
use log::*;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use crate::util::to_socket_addr;

pub const DEFAULT_DHT_BIND_ADDR: &str = "0.0.0.0:49737";

pub async fn run_bootstrap_node<A: ToSocketAddrs>(
    addr: Option<A>,
) -> io::Result<(SocketAddr, JoinHandle<io::Result<()>>)> {
    let node = if let Some(addr) = addr {
        BootstrapNode::with_addr(addr)?
    } else {
        BootstrapNode::default()
    };
    node.run().await
}

#[derive(Debug)]
pub struct BootstrapNode {
    config: DhtConfig,
    addr: Option<SocketAddr>,
}

impl Default for BootstrapNode {
    fn default() -> Self {
        Self::with_addr(DEFAULT_DHT_BIND_ADDR).unwrap()
    }
}

impl BootstrapNode {
    pub fn new(config: DhtConfig, addr: Option<SocketAddr>) -> Self {
        Self { config, addr }
    }

    pub fn with_addr<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let addr = to_socket_addr(addr)?;
        let config = DhtConfig::default()
            .empty_bootstrap_nodes()
            .set_ephemeral(false);
        Ok(Self::new(config, Some(addr)))
    }

    pub fn with_config<A: ToSocketAddrs>(config: DhtConfig, addr: Option<A>) -> io::Result<Self> {
        let addr = if let Some(addr) = addr {
            to_socket_addr(addr)?
        } else {
            to_socket_addr(DEFAULT_DHT_BIND_ADDR)?
        };
        Ok(Self::new(config, Some(addr)))
    }

    pub async fn run(self) -> io::Result<(SocketAddr, JoinHandle<io::Result<()>>)> {
        let Self { addr, config } = self;
        let addr = addr.unwrap_or(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            49737,
        ));
        let config = config.bind(addr).await.map_err(|(_, e)| e)?;
        let mut bs = HyperDht::with_config(config).await?;
        let addr = bs.local_addr()?;
        debug!("Running DHT on address: {}", addr);
        let task = async_std::task::spawn(async move {
            loop {
                let event = bs.next().await;
                trace!("[bootstrap node] event {:?}", event);
            }
        });
        Ok((addr, task))
    }
}
