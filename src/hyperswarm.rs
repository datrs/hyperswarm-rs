use async_std::channel;
use async_std::net::{TcpListener, TcpStream};
// use async_std::prelude::*;
use async_std::stream::StreamExt;
// use async_std::sync::{Arc, Mutex};
use async_std::task;
use async_std::task::JoinHandle;
use futures::future::FutureExt;
// use futures_channel::oneshot;
use futures_lite::{future, Stream};
use log::*;
// use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyperswarm_dht::{DhtConfig, HyperDht, HyperDhtEvent, IdBytes, Lookup, Peers, QueryOpts};

#[derive(Debug)]
pub enum SwarmEvent {
    Bootstrapped,
    Connection(TcpStream),
}

#[derive(Debug)]
pub enum Command {
    Join(IdBytes, JoinOpts),
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinOpts {
    pub lookup: bool,
    pub announce: bool,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    peers: Vec<Peers>,
    topic: IdBytes,
}

#[derive(Debug)]
pub struct Hyperswarm {
    task: Option<JoinHandle<io::Result<()>>>,
    command_tx: channel::Sender<Command>,
    events_rx: channel::Receiver<SwarmEvent>,
}

impl Hyperswarm {
    pub fn with_state(state: HyperDht) -> Self {
        let (command_tx, command_rx) = channel::unbounded();
        let (events_tx, events_rx) = channel::unbounded();
        let task = task::spawn(run_loop(state, command_rx, events_tx));
        Self {
            events_rx,
            command_tx,
            task: Some(task),
        }
    }

    pub async fn with_config(config: DhtConfig) -> io::Result<Self> {
        let state = HyperDht::with_config(config).await?;
        Ok(Self::with_state(state))
    }

    pub fn commands(&self) -> SwarmCommands {
        SwarmCommands(self.command_tx.clone())
    }
}

impl Stream for Hyperswarm {
    type Item = SwarmEvent;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.events_rx).poll_next(cx)
    }
}

#[derive(Debug)]
pub struct SwarmCommands(channel::Sender<Command>);

impl SwarmCommands {
    pub fn join(&self, topic: IdBytes, opts: JoinOpts) {
        self.0.try_send(Command::Join(topic, opts)).unwrap()
    }
}

async fn run_loop(
    mut state: HyperDht,
    mut command_rx: channel::Receiver<Command>,
    events_tx: channel::Sender<SwarmEvent>,
) -> io::Result<()> {
    enum Event {
        Dht(HyperDhtEvent),
        Command(Command),
    };

    let (connect_tx, connect_rx) = channel::unbounded();
    let connect_task = task::spawn(connect_loop(connect_rx, events_tx.clone()));
    // TODO: Allow to configure local port on which to accept peer connections.
    let (local_addr, accept_task) = accept_task(None, events_tx.clone()).await?;
    let local_port = local_addr.port() as u32;

    // wait for bootstrap event first.
    wait_for_bootstrap(&mut state).await?;
    events_tx.try_send(SwarmEvent::Bootstrapped).unwrap();

    while let Some(event) = future::race(
        state.next().map(|e| e.map(Event::Dht)),
        command_rx.next().map(|e| e.map(Event::Command)),
    )
    .await
    {
        match event {
            Event::Dht(event) => {
                debug!("swarm event: {:?}", event);
                match event {
                    HyperDhtEvent::Bootstrapped { .. } => {
                        // handled above, may not occur again?
                        unreachable!("received second bootstrap event");
                    }
                    HyperDhtEvent::AnnounceResult { .. } => {}
                    HyperDhtEvent::LookupResult { lookup, .. } => {
                        connect_tx.try_send(lookup).unwrap();
                    }
                    HyperDhtEvent::UnAnnounceResult { .. } => {}
                    _ => {}
                }
            }
            Event::Command(command) => {
                debug!("swarm command: {:?}", command);
                match command {
                    Command::Join(topic, join_opts) => {
                        let opts = QueryOpts {
                            topic,
                            port: Some(local_port),
                            local_addr: None,
                        };
                        if join_opts.announce {
                            state.announce(opts.clone());
                        }
                        if join_opts.lookup {
                            state.lookup(opts);
                        }
                    }
                }
            }
        }
    }

    connect_task.await;
    accept_task.await?;

    Ok(())
}

async fn connect_loop(
    mut connect_rx: channel::Receiver<Lookup>,
    events_tx: channel::Sender<SwarmEvent>,
) {
    while let Some(lookup) = connect_rx.next().await {
        let peers = lookup.remotes();
        // TODO: Connect over utp if tcp fails.
        for addr in peers {
            debug!("Connecting to peer {}", addr);
            let tcp_socket = TcpStream::connect(addr).await;
            // TODO: Also connect via UTP.
            // .race(UtpStream::connect(addr));
            match tcp_socket {
                Ok(stream) => {
                    debug!("Connected to peer {}", addr);
                    events_tx
                        .send(SwarmEvent::Connection(stream))
                        .await
                        .unwrap();
                }
                Err(err) => {
                    error!("Error connecting to peer {}: {}", addr, err);
                }
            }
        }
    }
}

async fn accept_task(
    local_port: Option<u32>,
    events_tx: channel::Sender<SwarmEvent>,
) -> io::Result<(SocketAddr, JoinHandle<io::Result<()>>)> {
    let port = local_port.unwrap_or(0);
    let address = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&address).await?;
    let local_addr = listener.local_addr()?;
    let accept_task = task::spawn(accept_loop(listener, events_tx));
    Ok((local_addr, accept_task))
}

async fn accept_loop(
    listener: TcpListener,
    events_tx: channel::Sender<SwarmEvent>,
) -> io::Result<()> {
    debug!(
        "accepting peer connections on tcp://{}",
        listener.local_addr()?
    );

    let mut incoming = listener.incoming();
    while let Some(Ok(stream)) = incoming.next().await {
        let peer_addr = stream.peer_addr().unwrap().to_string();
        debug!("Accepted connection from peer {}", peer_addr);
        events_tx
            .send(SwarmEvent::Connection(stream))
            .await
            .unwrap();
    }
    Ok(())
}

async fn wait_for_bootstrap(state: &mut HyperDht) -> io::Result<()> {
    let event = state.next().await;
    match event {
        Some(HyperDhtEvent::Bootstrapped { .. }) => {
            debug!("swarm bootstrapped!");
            Ok(())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Did not receive bootstrap as first event, abort.",
        )),
    }
}
