use async_std::stream::Stream;
use log::*;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::dht::DhtDiscovery;
use super::mdns::MdnsDiscovery;
use super::{Discovery, PeerInfo, Topic};
use crate::config::Config;

#[derive(Debug)]
pub struct CombinedDiscovery {
    dht: DhtDiscovery,
    mdns: MdnsDiscovery,
}

impl CombinedDiscovery {
    pub async fn bind(local_port: u16, config: Config) -> io::Result<Self> {
        let mdns = MdnsDiscovery::bind(local_port, config.clone()).await?;
        let dht = DhtDiscovery::bind(local_port, config).await?;
        Ok(Self { mdns, dht })
    }
}

impl Discovery for CombinedDiscovery {
    fn lookup(&mut self, topic: Topic) {
        debug!("lookup topic {}", hex::encode(topic));
        self.mdns.lookup(topic);
        self.dht.lookup(topic);
    }

    fn announce(&mut self, topic: Topic) {
        debug!("announce topic {}", hex::encode(topic));
        self.mdns.announce(topic);
        self.dht.announce(topic);
    }
}

impl Stream for CombinedDiscovery {
    type Item = io::Result<PeerInfo>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let next = Pin::new(&mut this.dht).poll_next(cx);
        if next.is_ready() {
            debug!("Found on DHT: {:?}", next);
            return next;
        }
        let next = Pin::new(&mut this.mdns).poll_next(cx);
        if next.is_ready() {
            debug!("Found on MDNS: {:?}", next);
            return next;
        }
        Poll::Pending
    }
}
