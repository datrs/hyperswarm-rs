use async_compat::Compat;
use futures::stream::FuturesUnordered;
use futures_lite::{AsyncRead, AsyncWrite, Stream};
use libutp_rs::{Connect as ConnectFut, UtpContext, UtpListener, UtpSocket};
use std::fmt;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Connection, Transport};

const PROTOCOL: &'static str = "utp";

pub struct UtpTransport {
    context: UtpContext,
    pending_connects: FuturesUnordered<ConnectFut>,
    incoming: UtpListener,
}

impl fmt::Debug for UtpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UtpTransport").finish()
    }
}

impl UtpTransport {
    pub async fn bind<A>(local_addr: A) -> io::Result<Self>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = local_addr.to_socket_addrs()?.next().unwrap();
        let context = UtpContext::bind(addr)?;
        let incoming = context.listener();
        Ok(Self {
            context,
            incoming,
            pending_connects: FuturesUnordered::new(),
        })
    }
}

impl Transport for UtpTransport {
    type Connection = UtpStream;
    fn connect(&mut self, peer_addr: SocketAddr) {
        eprintln!("UTP CONNECT START {}", peer_addr);
        let fut = self.context.connect(peer_addr);
        self.pending_connects.push(fut);
    }
}

impl Stream for UtpTransport {
    type Item = io::Result<Connection<<Self as Transport>::Connection>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let incoming = Pin::new(&mut self.incoming).poll_next(cx);
        if let Some(conn) = into_connection(incoming, false) {
            // eprintln!("UTP INCOMING {:?}", conn);
            return Poll::Ready(Some(conn));
        }

        let connect = Pin::new(&mut self.pending_connects).poll_next(cx);
        eprintln!(
            "UTP CONNECT {:?} (pending {})",
            connect,
            self.pending_connects.len()
        );
        if let Some(conn) = into_connection(connect, true) {
            // eprintln!("UTP CONNECT {:?}", conn);
            return Poll::Ready(Some(conn));
        }
        Poll::Pending
    }
}

fn into_connection(
    poll: Poll<Option<io::Result<UtpSocket>>>,
    is_initiator: bool,
) -> Option<io::Result<Connection<UtpStream>>> {
    match poll {
        Poll::Pending => None,
        Poll::Ready(None) => None,
        Poll::Ready(Some(Err(e))) => Some(Err(e)),
        Poll::Ready(Some(Ok(stream))) => {
            let stream = UtpStream::new(stream);
            let peer_addr = stream.peer_addr();
            let conn = Connection::new(stream, peer_addr, is_initiator, PROTOCOL.into());
            Some(Ok(conn))
        }
    }
}

pub struct UtpStream {
    inner: Compat<UtpSocket>,
}

impl fmt::Debug for UtpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UtpStream").finish()
    }
}

impl UtpStream {
    pub fn new(socket: UtpSocket) -> Self {
        Self {
            inner: Compat::new(socket),
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.inner.get_ref().peer_addr()
    }
}

impl AsyncRead for UtpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for UtpStream {
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

impl Clone for UtpStream {
    fn clone(&self) -> Self {
        Self {
            inner: Compat::new(self.inner.get_ref().clone()),
        }
    }
}
