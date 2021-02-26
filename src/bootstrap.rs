use async_std::net::ToSocketAddrs;
use async_std::stream::StreamExt;
use log::*;
use std::net::SocketAddr;

use hyperswarm_dht::{DhtConfig, HyperDht};

pub async fn bootstrap_dht<A: ToSocketAddrs>(local_addr: Option<A>) -> std::io::Result<SocketAddr> {
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
    async_std::task::spawn(async move {
        loop {
            bs.next().await;
        }
        // loop {
        // process each incoming message
        // let _event = bs.next().await;
        // debug!("bootstrap event: {:?}", event);
        // }
    });
    Ok(addr)
}
