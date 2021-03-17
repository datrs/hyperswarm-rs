use async_std::channel;
use async_std::stream::Stream;
use async_std::task::{Context, Poll};
use colmeia_hyperswarm_mdns::{self_id, Announcer, Locator};
use futures::FutureExt;
use futures_lite::ready;
// use log::*;
use std::convert::TryInto;
use std::fmt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::time::Duration;

use crate::Config;

use super::{Discovery, DiscoveryMethod, PeerInfo, Topic};

mod socket {
    use multicast_socket::MulticastSocket;
    use std::io;
    use std::net::{Ipv4Addr, SocketAddrV4};

    const MDNS_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

    pub fn create() -> io::Result<MulticastSocket> {
        let addr = SocketAddrV4::new(MDNS_IP, 5353);
        MulticastSocket::all_interfaces(addr)
    }
}

enum Command {
    Lookup(Topic),
    Announce(Topic),
}

pub type CommandFut = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;

pub struct MdnsDiscovery {
    announcer: Announcer,
    locator: Locator,
    // local_port: u16,
    // self_id: String,
    pending_commands_rx: channel::Receiver<Command>,
    pending_commands_tx: channel::Sender<Command>,
    pending_future: Option<Pin<Box<dyn Future<Output = io::Result<()>> + Send>>>,
}

impl fmt::Debug for MdnsDiscovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MdnsDiscovery").finish()
    }
}

impl MdnsDiscovery {
    pub async fn bind(local_port: u16, _config: Config) -> io::Result<Self> {
        let self_id = self_id();
        let socket = socket::create()?;
        let lookup_interval = Duration::from_secs(60);
        let locator = Locator::listen(socket, lookup_interval, self_id.as_bytes());
        let socket = socket::create()?;
        let announcer = Announcer::listen(socket, local_port, self_id.clone());
        let (pending_commands_tx, pending_commands_rx) = channel::unbounded();
        Ok(Self {
            locator,
            announcer,
            // self_id,
            // local_port,
            pending_commands_rx,
            pending_commands_tx,
            pending_future: None,
        })
    }
}

impl Discovery for MdnsDiscovery {
    fn lookup(&mut self, topic: Topic) {
        self.pending_commands_tx
            .try_send(Command::Lookup(topic))
            .unwrap();
    }

    fn announce(&mut self, topic: Topic) {
        self.pending_commands_tx
            .try_send(Command::Announce(topic))
            .unwrap();
    }
}

impl MdnsDiscovery {
    fn poll_pending_future(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(ref mut fut) = self.pending_future {
            let res = ready!(Pin::new(fut).poll(cx));
            self.pending_future = None;
            if let Err(e) = res {
                return Poll::Ready(Err(e));
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl Stream for MdnsDiscovery {
    type Item = io::Result<PeerInfo>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Err(e) = ready!(this.poll_pending_future(cx)) {
            return Poll::Ready(Some(Err(e)));
        }

        if let Poll::Ready(Some(command)) = Pin::new(&mut this.pending_commands_rx).poll_next(cx) {
            let fut = match command {
                Command::Lookup(topic) => {
                    let fut = this.locator.add_topic(topic.to_vec());
                    let fut = fut.map(|r| {
                        r.map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
                    });
                    let fut: CommandFut = fut.boxed();
                    fut
                }
                Command::Announce(topic) => {
                    let fut = this.announcer.add_topic(topic.to_vec());
                    let fut = fut.map(|r| {
                        r.map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
                    });
                    let fut: CommandFut = fut.boxed();
                    fut
                }
            };
            this.pending_future = Some(fut);
        }

        if let Err(e) = ready!(this.poll_pending_future(cx)) {
            return Poll::Ready(Some(Err(e)));
        }

        let _ = Pin::new(&mut this.announcer).poll_next(cx);

        let res = ready!(Pin::new(&mut this.locator).poll_next(cx));
        if let Some((topic, peer_addr)) = res {
            let topic = topic.try_into();
            if let Ok(topic) = topic {
                Poll::Ready(Some(Ok(PeerInfo::new(
                    peer_addr,
                    Some(topic),
                    DiscoveryMethod::Mdns,
                ))))
            } else {
                Poll::Ready(Some(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Received invalid topic",
                ))))
            }
        } else {
            Poll::Pending
        }
    }
}
