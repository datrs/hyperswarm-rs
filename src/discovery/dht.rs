use async_std::stream::Stream;
use futures_lite::ready;
use hyperswarm_dht::{HyperDht, HyperDhtEvent, QueryOpts};
use log::*;
use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Discovery, DiscoveryMethod, PeerInfo, Topic};

pub use hyperswarm_dht::DhtConfig;

// #[derive(Debug)]
pub struct DhtDiscovery {
    state: HyperDht,
    bootstrapped: bool,
    announce_port: u16,
    pending_commands: VecDeque<Command>,
    pending_events: VecDeque<PeerInfo>,
}

impl fmt::Debug for DhtDiscovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DhtDiscovery")
            .field("bootstrapped", &self.bootstrapped)
            .field("announce_port", &self.announce_port)
            .finish()
    }
}

#[derive(Debug)]
enum Command {
    Lookup(QueryOpts),
    Announce(QueryOpts),
}

impl DhtDiscovery {
    pub async fn bind(config: DhtConfig, announce_port: u16) -> io::Result<Self> {
        let state = HyperDht::with_config(config).await?;
        let this = Self {
            state,
            announce_port,
            bootstrapped: false,
            pending_commands: VecDeque::new(),
            pending_events: VecDeque::new(),
        };
        Ok(this)
    }

    fn execute_pending_commands(&mut self) {
        while let Some(command) = self.pending_commands.pop_front() {
            match command {
                Command::Announce(opts) => self.state.announce(opts),
                Command::Lookup(opts) => self.state.lookup(opts),
            };
        }
    }
}

impl Discovery for DhtDiscovery {
    fn lookup(&mut self, topic: Topic) {
        let opts = QueryOpts {
            topic: topic.into(),
            port: Some(self.announce_port as u32),
            local_addr: None,
        };
        self.pending_commands.push_back(Command::Lookup(opts))
    }

    fn announce(&mut self, topic: Topic) {
        let opts = QueryOpts {
            topic: topic.into(),
            port: Some(self.announce_port as u32),
            local_addr: None,
        };
        self.pending_commands.push_back(Command::Announce(opts))
    }
}

impl Stream for DhtDiscovery {
    type Item = io::Result<PeerInfo>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(event) = self.pending_events.pop_front() {
                return Poll::Ready(Some(Ok(event)));
            }

            if self.bootstrapped {
                self.execute_pending_commands();
            }

            let event = ready!(Pin::new(&mut self.state).poll_next(cx));
            trace!("DHT event: {:?}", event);
            let event = event.unwrap();
            match event {
                HyperDhtEvent::Bootstrapped { .. } => {
                    debug!("DHT bootstrapped!");
                    self.bootstrapped = true;
                }
                HyperDhtEvent::AnnounceResult { .. } => {}
                HyperDhtEvent::LookupResult { lookup, .. } => {
                    let topic = lookup.topic.0;
                    let peers = lookup.remotes();
                    for addr in peers {
                        let info = PeerInfo::new(*addr, Some(topic), DiscoveryMethod::Dht);
                        self.pending_events.push_back(info);
                    }
                }
                HyperDhtEvent::UnAnnounceResult { .. } => {}
                _ => {}
            }
        }
    }
}
