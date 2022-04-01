pub use async_std::net::TcpStream;
use async_std::net::{SocketAddr, TcpListener};
use async_std::stream::Stream;
use futures::stream::FuturesUnordered;
use futures_lite::{ready, Future};
use std::fmt;
use std::io;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Connection, Transport};

pub type ConnectFut = Pin<Box<dyn Future<Output = io::Result<TcpStream>> + Send + 'static>>;

const PROTOCOL: &'static str = "tcp";

#[derive(Debug)]
pub struct TcpTransport {
    addr: SocketAddr,
    incoming: TcpIncoming,
    pending_connects: FuturesUnordered<ConnectFut>,
}

impl TcpTransport {
    pub async fn bind<A>(local_addr: A) -> io::Result<Self>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = local_addr.to_socket_addrs()?.next().unwrap();
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let incoming = TcpIncoming::new(listener)?;
        log::debug!("TCP socket listening on {}", addr);
        Ok(Self {
            addr,
            incoming,
            pending_connects: FuturesUnordered::new(),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Transport for TcpTransport {
    type Connection = TcpStream;

    fn connect(&mut self, peer_addr: SocketAddr) {
        let fut = TcpStream::connect(peer_addr);
        // let fut = connect_delayed(peer_addr);
        self.pending_connects.push(Box::pin(fut));
    }
}

impl Stream for TcpTransport {
    type Item = io::Result<Connection<<Self as Transport>::Connection>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let incoming = Pin::new(&mut self.incoming).poll_next(cx);
        if let Some(conn) = into_connection(incoming, false) {
            return Poll::Ready(Some(conn));
        }

        let connect = Pin::new(&mut self.pending_connects).poll_next(cx);
        if let Some(conn) = into_connection(connect, true) {
            return Poll::Ready(Some(conn));
        }
        Poll::Pending
    }
}

fn into_connection(
    poll: Poll<Option<io::Result<TcpStream>>>,
    is_initiator: bool,
) -> Option<io::Result<Connection<TcpStream>>> {
    match poll {
        Poll::Pending => None,
        Poll::Ready(None) => None,
        Poll::Ready(Some(Err(e))) => Some(Err(e)),
        Poll::Ready(Some(Ok(stream))) => {
            let peer_addr = stream.peer_addr().unwrap();
            let conn = Connection::new(stream, peer_addr, is_initiator, PROTOCOL.into());
            Some(Ok(conn))
        }
    }
}

pub struct TcpIncoming {
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

// async fn connect_delayed(peer_addr: SocketAddr) -> io::Result<TcpStream> {
//     timeout(100).await;
//     TcpStream::connect(peer_addr).await
// }

// async fn timeout(ms: u64) {
//     let _ = async_std::future::timeout(
//         std::time::Duration::from_millis(ms),
//         futures::future::pending::<()>(),
//     )
//     .await;
// }
