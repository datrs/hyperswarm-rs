use async_std::channel;
use futures_lite::{ready, AsyncRead, AsyncWrite, FutureExt, Stream};
use log::*;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::config::{Config, TopicConfig};
use crate::discovery::Topic;
use crate::discovery::{combined::CombinedDiscovery, Discovery};
use crate::transport::Incoming;
use crate::transport::{
    combined::{CombinedStream, CombinedTransport},
    Transport,
};

pub type ConnectFut = Pin<Box<dyn Future<Output = io::Result<HyperswarmStream>> + Send + 'static>>;
type ConfigureCommand = (Topic, TopicConfig);

pub struct Hyperswarm {
    topics: HashMap<Topic, TopicConfig>,
    discovery: CombinedDiscovery,
    transport: CombinedTransport,
    incoming: Incoming<CombinedStream>,
    command_tx: channel::Sender<ConfigureCommand>,
    command_rx: channel::Receiver<ConfigureCommand>,
    pending_connects: Vec<ConnectFut>,
}
impl fmt::Debug for Hyperswarm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Driver")
            .field("topics", &self.topics)
            .field("local_addr", &self.incoming.local_addr())
            // .field("discovery", &self.discovery)
            // .field("transport", &self.transport)
            // .field("incoming", &self.incoming)
            .field(
                "pending_connects",
                &format!("<{}>", self.pending_connects.len()),
            )
            .finish()
    }
}

impl Hyperswarm {
    pub async fn bind(config: Config) -> io::Result<Self> {
        let local_addr = "localhost:0";
        let mut transport = CombinedTransport::new();
        let incoming = transport.listen(local_addr).await?;
        let local_addr = incoming.local_addr();
        let port = local_addr.port();
        let discovery = CombinedDiscovery::listen(port, config).await?;
        let (command_tx, command_rx) = channel::unbounded::<ConfigureCommand>();
        Ok(Self {
            topics: HashMap::new(),
            discovery,
            transport,
            incoming,
            pending_connects: vec![],
            command_tx,
            command_rx,
        })
    }

    pub fn configure(&mut self, topic: Topic, config: TopicConfig) {
        let old = self.topics.remove(&topic).unwrap_or_default();
        if config.announce && !old.announce {
            self.discovery.announce(topic);
        }
        if config.lookup && !old.lookup {
            self.discovery.lookup(topic);
        }
        // TODO: unannounce and stop-lookup
        self.topics.insert(topic, config);
    }

    fn poll_pending_connects(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<io::Result<HyperswarmStream>>> {
        let mut i = 0;
        let mut iter = self.pending_connects.iter_mut();
        while let Some(ref mut fut) = iter.next() {
            let res = Pin::new(fut).poll(cx);
            if let Poll::Ready(res) = res {
                self.pending_connects.remove(i);
                return Poll::Ready(Some(res));
            }
            i += 1;
        }
        Poll::Pending
    }

    pub fn handle(&self) -> SwarmHandle {
        SwarmHandle {
            command_tx: self.command_tx.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwarmHandle {
    command_tx: channel::Sender<ConfigureCommand>,
}

impl SwarmHandle {
    pub fn configure(&self, topic: Topic, config: TopicConfig) {
        self.command_tx.try_send((topic, config)).unwrap();
    }
}

impl Stream for Hyperswarm {
    type Item = io::Result<HyperswarmStream>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Poll pending connect futures.
        let res = this.poll_pending_connects(cx);
        if res.is_ready() {
            return res;
        }

        // Poll commands.
        while let Poll::Ready(Some((topic, config))) = Pin::new(&mut this.command_rx).poll_next(cx)
        {
            this.configure(topic, config);
        }

        // Poll discovery results.
        let discovery = Pin::new(&mut this.discovery).poll_next(cx);
        match discovery {
            Poll::Pending | Poll::Ready(None) => {}
            Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
            Poll::Ready(Some(Ok(peer_info))) => {
                debug!("discovery: {:?}", peer_info);
                let fut = connect(this.transport.clone(), peer_info.addr());
                let mut fut = fut.boxed();
                let res = fut.poll(cx);
                if let Poll::Ready(res) = res {
                    return Poll::Ready(Some(res));
                } else {
                    this.pending_connects.push(fut);
                }
            }
        }

        // Poll incoming streams.
        let stream = ready!(Pin::new(&mut this.incoming).poll_next(cx));
        let stream = stream.unwrap();
        let stream = stream.map(|stream| HyperswarmStream::new(stream, false));
        Poll::Ready(Some(stream))
    }
}

async fn connect(
    mut transport: CombinedTransport,
    peer_addr: SocketAddr,
) -> io::Result<HyperswarmStream> {
    transport
        .connect(peer_addr)
        .await
        .map(|stream| HyperswarmStream::new(stream, true))
}

#[derive(Debug)]
pub struct HyperswarmStream {
    inner: CombinedStream,
    peer_addr: SocketAddr,
    is_initiator: bool,
}

impl HyperswarmStream {
    pub fn new(inner: CombinedStream, is_initiator: bool) -> Self {
        Self {
            peer_addr: inner.peer_addr(),
            inner,
            is_initiator,
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn protocol(&self) -> String {
        self.inner.protocol()
    }

    pub fn is_initiator(&self) -> bool {
        self.is_initiator
    }

    pub fn get_ref(&self) -> &CombinedStream {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut CombinedStream {
        &mut self.inner
    }
}

impl AsyncRead for HyperswarmStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for HyperswarmStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[cfg(test)]
mod test {
    use super::{Config, Hyperswarm, HyperswarmStream, Topic, TopicConfig};
    use crate::run_bootstrap_node;
    use async_std::task::{self, JoinHandle};
    use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
    use std::io::Result;
    use std::net::SocketAddr;

    fn drive_stream(mut stream: HyperswarmStream, name: &'static str) {
        task::spawn(async move {
            stream
                .write_all(format!("hello, here is {}", name).as_bytes())
                .await
                .unwrap();
            let mut buf = vec![0u8; 64];
            loop {
                match stream.read(&mut buf[..]).await {
                    Err(e) => eprintln!("[{}] ERROR: {}", name, e),
                    Ok(0) => {
                        eprintln!("[{}] stream closed", name,);
                        return;
                    }
                    Ok(n) => eprintln!(
                        "[{}] RECV: {:?}",
                        name,
                        String::from_utf8(buf[..n].to_vec())
                    ),
                };
            }
        });
    }

    #[async_std::test]
    async fn test_driver() -> Result<()> {
        let (bs_addr, bs_task) = run_bootstrap_node::<SocketAddr>(None).await?;
        eprintln!("ok go");
        let mut config = Config::default().set_bootstrap_nodes(vec![bs_addr]);
        let mut swarm_a = Hyperswarm::bind(config.clone()).await?;
        let mut swarm_b = Hyperswarm::bind(config).await?;
        eprintln!("A {:?}", swarm_a);
        eprintln!("B {:?}", swarm_b);

        let topic = [0u8; 32];
        let config = TopicConfig::both();
        swarm_a.configure(topic, config.clone());
        swarm_b.configure(topic, config.clone());

        // let topic = [1u8; 32];
        // let config = TopicConfig::both();
        // swarm_a.configure(topic, config.clone());
        // swarm_b.configure(topic, config.clone());

        let task_a = task::spawn(async move {
            while let Some(stream) = swarm_a.next().await {
                let stream = stream.unwrap();
                eprintln!("A incoming: {:?}", stream);
                drive_stream(stream, "alice");
            }
        });
        let task_b = task::spawn(async move {
            while let Some(stream) = swarm_b.next().await {
                let stream = stream.unwrap();
                eprintln!("B incoming: {:?}", stream);
                drive_stream(stream, "bob");
            }
        });

        task_a.await;
        task_b.await;
        bs_task.await?;
        Ok(())
    }
}
