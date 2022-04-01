use async_std::channel::{Receiver, Sender};
use async_std::io::{self, ReadExt, WriteExt};
use async_std::stream::StreamExt;
use async_std::sync::Mutex;
use async_std::task::{self, JoinHandle};
use clap::Parser;
use futures::future::{select, Either};
use std::net::SocketAddr;
use std::sync::Arc;

use hyperswarm::{
    hash_topic, BootstrapNode, Config, DhtConfig, Hyperswarm, HyperswarmStream, TopicConfig,
};

const DISCOVERY_NS_BUF: &[u8] = b"hyperchat:v0";

#[derive(Parser, Debug)]
struct Opts {
    /// Set bootstrap addresses
    #[clap(short, long)]
    pub bootstrap: Vec<SocketAddr>,

    /// Address to bind DHT node to (default: 0.0.0.0:49737).
    #[clap(long)]
    pub bind: Option<SocketAddr>,

    /// Command to run
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    /// Run a DHT bootstrap node.
    Bootstrap,
    /// Join the swarm and connect to peers.
    Join(JoinOpts),
}

#[derive(Parser, Debug)]
pub struct JoinOpts {
    /// Set topics
    #[clap(short, long)]
    pub topics: Vec<String>,
    /// Set name to send to peers in hello message
    #[clap(short, long)]
    pub name: Option<String>,
}

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts: Opts = Opts::parse();
    match opts.command {
        Command::Bootstrap => {
            let config = DhtConfig::default();
            let config = if !opts.bootstrap.is_empty() {
                config.set_bootstrap_nodes(&opts.bootstrap)
            } else {
                config.empty_bootstrap_nodes()
            };
            let node = BootstrapNode::new(config, opts.bind);
            let (addr, task) = node.run().await?;
            eprintln!("Running bootstrap node on {:?}", addr);
            task.await?;
        }
        Command::Join(join_opts) => {
            let config = Config::default();
            let config = if !opts.bootstrap.is_empty() {
                config.set_bootstrap_nodes(&opts.bootstrap)
            } else {
                config
            };
            let name = join_opts.name.unwrap_or_else(random_name);
            eprintln!("your name: {}", name);

            // Bind and open swarm.
            let swarm = Hyperswarm::bind(config).await?;
            // Configure swarm and listen on topics.
            let handle = swarm.handle();
            let config = TopicConfig::announce_and_lookup();
            for topic in join_opts.topics {
                let hash = hash_topic(DISCOVERY_NS_BUF, topic.as_bytes());
                handle.configure(hash, config.clone());
                eprintln!("join topic \"{}\": {}", topic, hex::encode(hash));
            }

            // Open broadcaster.
            let initial_message = name.as_bytes().to_vec();
            let broadcaster = HyperBroadcast::new(initial_message);

            // Start the broadcast loops.
            let (task, mut incoming_rx) = broadcaster.run(swarm).await;

            // Print incoming messages.
            task::spawn(async move {
                let mut name = None;
                while let Some(message) = incoming_rx.next().await {
                    let content = String::from_utf8(message.content)
                        .unwrap_or_else(|_| "<invalid utf8>".to_string());
                    match name.as_ref() {
                        None => {
                            println!("[{}] is now known as `{}`", message.from, content);
                            name = Some(content);
                        }
                        Some(name) => {
                            println!("[{}] <{}> {}", message.from, name, content);
                        }
                    }
                }
            });

            // Read outgoing messages from stdin.
            {
                let broadcaster = broadcaster.clone();
                task::spawn(async move {
                    let stdin = io::stdin();
                    loop {
                        let mut line = String::new();
                        stdin.read_line(&mut line).await.unwrap();
                        broadcaster.broadcast(line).await;
                    }
                });
            }

            // Wait for the broadcast task until error or forever.
            task.await?;
        }
    };
    Ok(())
}

#[derive(Clone)]
struct HyperBroadcast {
    initial_message: Vec<u8>,
    peers: Arc<Mutex<Vec<Peer>>>,
}

impl HyperBroadcast {
    pub fn new(initial_message: Vec<u8>) -> Self {
        Self {
            initial_message,
            peers: Default::default(),
        }
    }

    pub async fn broadcast(&self, message: String) {
        let mut peers = self.peers.lock().await;
        for peer in peers.iter_mut() {
            match peer.send_message(message.clone()).await {
                Ok(_) => log::debug!("Message sent to {}", peer.id()),
                Err(err) => log::error!("Error sending message to {}: {}", peer.id(), err),
            }
        }
    }

    pub async fn run(
        &self,
        mut swarm: Hyperswarm,
    ) -> (JoinHandle<anyhow::Result<()>>, Receiver<IncomingMessage>) {
        let (incoming_tx, incoming_rx) = async_std::channel::unbounded();
        let peers = self.peers.clone();
        let initial_message = self.initial_message.clone();
        let task = task::spawn(async move {
            while let Some(stream) = swarm.next().await {
                let stream = stream?;

                // Open channels.
                let (outbound_tx, outbound_rx) = async_std::channel::unbounded();
                let (inbound_tx, mut inbound_rx) = async_std::channel::unbounded();

                // Queue initial message.
                outbound_tx.send(initial_message.clone()).await?;

                // Construct peer info.
                let peer = Peer {
                    outbound_tx,
                    addr: stream.peer_addr(),
                    protocol: stream.protocol().to_string(),
                };
                let peer_id = peer.id();
                println!("[{}] connected", peer_id);

                // Spawn loop to send and receive messages.
                {
                    let peers = peers.clone();
                    task::spawn(async move {
                        if let Err(err) = connection_loop(stream, inbound_tx, outbound_rx).await {
                            // On error in peer connection, remove from active peers.
                            println!("[{}] disconnected", peer_id);
                            log::debug!("[{}] error: {}", peer_id, err);
                            peers.lock().await.retain(|peer| peer.id() != peer_id);
                        }
                    });
                }

                // Spawn loop to forward incoming messages.
                {
                    let peer_id = peer.id();
                    let incoming_tx = incoming_tx.clone();
                    task::spawn(async move {
                        while let Some(message) = inbound_rx.next().await {
                            let message = IncomingMessage {
                                from: peer_id.clone(),
                                content: message,
                            };
                            incoming_tx.send(message).await.unwrap();
                        }
                    });
                }

                // Save peer for broadcasting.
                peers.lock().await.push(peer);
            }
            Ok(())
        });
        (task, incoming_rx)
    }
}

type PeerId = String;

struct IncomingMessage {
    from: PeerId,
    content: Vec<u8>,
}

struct Peer {
    outbound_tx: Sender<Vec<u8>>,
    addr: SocketAddr,
    protocol: String,
}

impl Peer {
    pub fn id(&self) -> PeerId {
        format!("{}:{}", self.protocol, self.addr)
    }

    pub async fn send_message(&self, message: String) -> anyhow::Result<()> {
        let message = message.as_bytes().to_vec();
        self.outbound_tx.send(message).await?;
        Ok(())
    }
}

async fn connection_loop(
    mut stream: HyperswarmStream,
    inbound_tx: Sender<Vec<u8>>,
    mut outbound_rx: Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    let mut len_buf = [0u8; 4];
    loop {
        // Incoming message.
        let left = stream.read_exact(&mut len_buf);
        // Outgoing message.
        let right = outbound_rx.next();
        match select(left, right).await {
            Either::Left((res, _)) => match res {
                Ok(_) => {
                    let len = u32::from_be_bytes(len_buf);
                    let mut buf = vec![0u8; len as usize];
                    stream.read_exact(&mut buf).await?;
                    inbound_tx.send(buf).await?;
                }
                Err(err) => return Err(err.into()),
            },
            Either::Right((message, _)) => match message {
                Some(message) => {
                    let mut buf = Vec::new();
                    buf.extend((message.len() as u32).to_be_bytes());
                    buf.extend(&message[..]);
                    stream.write_all(&buf).await?;
                }
                None => return Err(anyhow::anyhow!("Remote connection closed?")),
            },
        }
    }
}

pub fn random_name() -> String {
    let bytes = rand::random::<[u8; 4]>();
    hex::encode(&bytes)
}
