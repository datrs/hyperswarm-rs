use async_std::channel;
use futures::{AsyncRead, AsyncWrite, StreamExt};
use futures_lite::Stream;
use log::*;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{config::{DhtOptions, TopicConfig}, transport::DynamicConnection};
use crate::discovery::Topic;
use crate::discovery::{combined::CombinedDiscovery, Discovery};
use crate::transport::{
    combined::{CombinedStream, CombinedTransport},
    Connection, Transport,
};

type ConfigureCommand = (Topic, TopicConfig);

pub struct Hyperswarm {
    topics: HashMap<Topic, TopicConfig>,
    discovery: Box<dyn Discovery + Send + Unpin>,
    // Error: Transport requires specifing Connection
    transport: Box<
        dyn Transport<
                Connection = Box<dyn DynamicConnection>,
                Item = io::Result<Box<dyn DynamicConnection>>
            > + Send
            + Unpin,
    >,
    command_tx: channel::Sender<ConfigureCommand>,
    command_rx: channel::Receiver<ConfigureCommand>,
}
impl fmt::Debug for Hyperswarm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hyperswarm")
            .field("topics", &self.topics)
            .finish()
    }
}

impl Hyperswarm {
    pub async fn listen() -> io::Result<impl Transport> {
        let local_addr = "localhost:0";
        CombinedTransport::bind(local_addr).await
    }

    pub fn with(
        transport: impl Transport + Send + Unpin,
        discovery: impl Discovery + Send + Unpin,
    ) -> Self {
        let (command_tx, command_rx) = channel::unbounded::<ConfigureCommand>();

        Self {
            topics: HashMap::new(),
            discovery: Box::new(discovery),
            transport: Box::new(transport),
            command_tx,
            command_rx,
        }
    }

    pub async fn bind(config: DhtOptions) -> io::Result<Self> {
        let local_addr = "localhost:0";

        let transport = CombinedTransport::bind(local_addr).await?;
        let local_addr = transport.local_addr();
        let port = local_addr.port();
        let discovery = CombinedDiscovery::bind(port, config).await?;

        Ok(Self::with(transport, discovery))
    }

    pub fn configure(&mut self, topic: Topic, config: TopicConfig) {
        let old = self.topics.remove(&topic).unwrap_or_default();
        debug!("configure swarm: {} {:?}", hex::encode(topic), config);
        if config.announce && !old.announce {
            self.discovery.announce(topic);
        }
        if config.lookup && !old.lookup {
            self.discovery.lookup(topic);
        }
        // TODO: unannounce and stop-lookup
        self.topics.insert(topic, config);
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
    type Item = io::Result<Box<dyn DynamicConnection>>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Poll new connections.
        let res = Pin::new(&mut this.transport).poll_next(cx);
        if let Poll::Ready(Some(res)) = res {
            debug!("new connection: {:?}", res);
            return Poll::Ready(Some(res));
        }

        // Poll commands.
        while let Poll::Ready(Some((topic, config))) = Pin::new(&mut this.command_rx).poll_next(cx)
        {
            this.configure(topic, config);
        }

        // Poll discovery results.
        let discovery = Pin::new(&mut this.discovery).poll_next_unpin(cx);
        match discovery {
            Poll::Pending | Poll::Ready(None) => {}
            Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
            Poll::Ready(Some(Ok(peer_info))) => {
                this.transport.connect(peer_info.addr());
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod test {
    use super::{DhtOptions, Hyperswarm, TopicConfig};
    use crate::run_bootstrap_node;
    use async_std::channel;
    use async_std::task;
    use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
    use std::io::Result;
    use std::net::SocketAddr;

    #[async_std::test]
    async fn test_swarm() -> Result<()> {
        env_logger::init();
        let (bs_addr, _bs_task) = run_bootstrap_node::<SocketAddr>(None).await?;
        // eprintln!("bootstrap node on {}", bs_addr);
        let config = DhtOptions::default().set_bootstrap_nodes(Some(vec![bs_addr]));
        let mut swarm_a = Hyperswarm::bind(config.clone()).await?;
        // eprintln!("A {:?}", swarm_a);
        let mut swarm_b = Hyperswarm::bind(config).await?;
        // eprintln!("B {:?}", swarm_b);

        let topic = [0u8; 32];
        let config = TopicConfig::both();
        swarm_a.configure(topic, config.clone());
        swarm_b.configure(topic, config.clone());

        let (done1_tx, mut done1_rx) = channel::bounded(1);
        let task_a = task::spawn(async move {
            while let Some(stream) = swarm_a.next().await {
                eprintln!("A: NEW STREAM {:?}", stream);
                let mut stream = stream.unwrap();
                let done_tx = done1_tx.clone();
                task::spawn(async move {
                    let mut buf = vec![0u8; 1];
                    // eprintln!(
                    //     "A WRITE start: method {} init {}",
                    //     stream.protocol(),
                    //     stream.is_initiator()
                    // );
                    stream.write_all(b"A").await.unwrap();
                    // eprintln!(
                    //     "A WROTE fin: method {} init {}",
                    //     stream.protocol(),
                    //     stream.is_initiator()
                    // );
                    let n = stream.read(&mut buf).await.unwrap();
                    assert_eq!(n, 1);
                    assert_eq!(&buf[..n], b"B");
                    // eprintln!("A res {:?} buf {:?}", n, &buf);
                    let _ = done_tx.send(()).await;
                });
            }
        });

        // wait 50ms so that the first swarm properly announced to the DHT.
        timeout(50).await;

        let (done2_tx, mut done2_rx) = channel::bounded(1);
        let task_b = task::spawn(async move {
            while let Some(stream) = swarm_b.next().await {
                let mut stream = stream.unwrap();
                let done_tx = done2_tx.clone();
                task::spawn(async move {
                    let mut buf = vec![0u8, 1];
                    // eprintln!(
                    //     "B WRITE start: method {} init {}",
                    //     stream.protocol(),
                    //     stream.is_initiator()
                    // );
                    stream.write_all(b"B").await.unwrap();
                    // eprintln!(
                    //     "B WRITE fin: method {} init {}",
                    //     stream.protocol(),
                    //     stream.is_initiator()
                    // );
                    let n = stream.read(&mut buf).await.unwrap();
                    assert_eq!(n, 1);
                    assert_eq!(&buf[..n], b"A");
                    // eprintln!("B res {:?} buf {:?}", n, &buf);
                    let _ = done_tx.send(()).await;
                    // done_tx.send(()).await.unwrap();
                });
            }
        });

        done1_rx.next().await.unwrap();
        done2_rx.next().await.unwrap();
        task_a.cancel().await;
        task_b.cancel().await;
        // task_a.await;
        // task_b.await;
        Ok(())
    }

    async fn timeout(ms: u64) {
        let _ = async_std::future::timeout(
            std::time::Duration::from_millis(ms),
            futures::future::pending::<()>(),
        )
        .await;
    }
}
