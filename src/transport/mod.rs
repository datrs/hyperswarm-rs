use futures::io::{AsyncRead, AsyncWrite};
use futures::stream::Stream;
use std::fmt::{self, Debug};
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

pub mod combined;
pub mod tcp;

#[cfg(feature = "transport_utp")]
pub mod utp;

pub trait Transport:
    Stream<Item = io::Result<Connection<<Self as Transport>::Connection>>>
{
    type Connection: AsyncRead + AsyncWrite + Send + std::fmt::Debug;
    fn connect(&mut self, peer_addr: SocketAddr);
    // fn poll_next(
    //     self: Pin<&mut Self>,
    //     cx: &mut Context<'_>,
    // ) -> Poll<Option<io::Result<Connection<Self::Connection>>>>;
}

#[derive(Debug, Clone)]
pub struct Connection<T>
where
    T: Debug,
{
    inner: T,
    peer_addr: SocketAddr,
    is_initiator: bool,
    protocol: String,
}

impl<T> Connection<T>
where
    T: Debug + AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(inner: T, peer_addr: SocketAddr, is_initiator: bool, protocol: String) -> Self {
        Self {
            inner,
            peer_addr,
            is_initiator,
            protocol,
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn is_initiator(&self) -> bool {
        self.is_initiator
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn into_parts(self) -> (T, SocketAddr, bool, String) {
        (self.inner, self.peer_addr, self.is_initiator, self.protocol)
    }
}

impl<T> fmt::Display for Connection<T>
where
    T: Debug + AsyncRead + AsyncWrite + Unpin,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.protocol(), self.peer_addr())
    }
}

impl<T> AsyncRead for Connection<T>
where
    T: AsyncRead + Unpin + Debug,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for Connection<T>
where
    T: AsyncWrite + Unpin + Debug,
{
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
