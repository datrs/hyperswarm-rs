use async_std::net::ToSocketAddrs;
use async_std::stream::StreamExt;
use async_std::task::JoinHandle;
use log::*;
use std::io;
use std::net::SocketAddr;

use hyperswarm_dht::{DhtConfig, HyperDht};

pub async fn run_bootstrap_node<A: ToSocketAddrs>(
    local_addr: Option<A>,
) -> io::Result<(SocketAddr, JoinHandle<io::Result<()>>)> {
    let config = DhtConfig::default()
        .empty_bootstrap_nodes()
        .set_ephemeral(false);
    let config = if let Some(addr) = local_addr {
        config.bind(addr).await.map_err(|(_, e)| e)?
    } else {
        config
    };
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
