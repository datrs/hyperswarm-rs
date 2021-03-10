pub use async_std::net::TcpStream;
use async_std::net::{SocketAddr, TcpListener};
use async_std::stream::Stream;
use async_trait::async_trait;
use futures_lite::{ready, Future};
use std::fmt;
use std::io;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Incoming, Transport};

#[derive(Clone, Debug)]
pub struct TcpTransport;

#[async_trait]
impl Transport for TcpTransport {
    type Connection = TcpStream;
    fn new() -> Self {
        Self {}
    }
    async fn listen<A>(&mut self, local_addr: A) -> io::Result<Incoming<Self::Connection>>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = local_addr.to_socket_addrs()?.next().unwrap();
        let listener = TcpListener::bind(addr).await?;
        let incoming = TcpIncoming::new(listener)?;
        let local_addr = incoming.local_addr()?;
        let incoming = Incoming::new(incoming, local_addr);
        Ok(incoming)
    }
    async fn connect<A>(&mut self, peer_addr: A) -> io::Result<Self::Connection>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = peer_addr.to_socket_addrs()?.next().unwrap();
        let stream = TcpStream::connect(addr).await?;
        Ok(stream)
    }
}

pub struct TcpIncoming {
    // listener: TcpListener,
    local_addr: SocketAddr,
    accept: Pin<
        Box<dyn Future<Output = (TcpListener, io::Result<(TcpStream, SocketAddr)>)> + Send + Sync>,
    >,
}

impl TcpIncoming {
    pub fn new(listener: TcpListener) -> io::Result<Self> {
        let local_addr = listener.local_addr()?;
        let accept = Box::pin(accept(listener));
        Ok(Self { local_addr, accept })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }
}

impl Stream for TcpIncoming {
    type Item = io::Result<TcpStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (listener, res) = ready!(self.accept.as_mut().poll(cx));
        self.accept = Box::pin(accept(listener));
        let res = res.map(|r| r.0);
        Poll::Ready(Some(res))
    }
}

async fn accept(listener: TcpListener) -> (TcpListener, io::Result<(TcpStream, SocketAddr)>) {
    let result = listener.accept().await;
    (listener, result)
}

impl fmt::Debug for TcpIncoming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpIncoming")
            .field("local_addr", &self.local_addr)
            .finish()
    }
}
