use async_std::stream::Stream;
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
    pub async fn bind(config: Config, announce_port: u16) -> io::Result<Self> {
        let mdns = MdnsDiscovery::bind(config.mdns.unwrap_or_default(), announce_port).await?;
        let dht = DhtDiscovery::bind(config.dht.unwrap_or_default(), announce_port).await?;
        Ok(Self { mdns, dht })
    }
}

impl Discovery for CombinedDiscovery {
    fn lookup(&mut self, topic: Topic) {
        self.mdns.lookup(topic);
        self.dht.lookup(topic);
    }

    fn announce(&mut self, topic: Topic) {
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
            return next;
        }
        let next = Pin::new(&mut this.mdns).poll_next(cx);
        if next.is_ready() {
            return next;
        }
        Poll::Pending
    }
}
