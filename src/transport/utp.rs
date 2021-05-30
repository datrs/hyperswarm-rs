use async_std_utp::{UtpListener, UtpSocket};
use futures::FutureExt;
use futures::{future::BoxFuture, stream::FuturesUnordered, Stream};
use std::fmt;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Connection, Transport};

pub(crate) use async_std_utp::UtpStream;

const PROTOCOL: &'static str = "utp";

type ConnectFut = BoxFuture<'static, io::Result<UtpStream>>;

pub struct UtpTransport {
    pending_connects: FuturesUnordered<ConnectFut>,
    context: UtpListener,
    incoming: Option<BoxFuture<'static, io::Result<(UtpSocket, SocketAddr)>>>,
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
        let context = UtpListener::bind(addr).await?;
        Ok(Self {
            context,
            pending_connects: FuturesUnordered::new(),
            incoming: None,
        })
    }
}

impl Transport for UtpTransport {
    type Connection = UtpStream;
    fn connect(&mut self, peer_addr: SocketAddr) {
        let fut = async move {
            let socket = UtpSocket::connect(peer_addr).await?;
            Ok(UtpStream::from(socket))
        }
        .boxed();
        self.pending_connects.push(fut);
    }
}

impl Stream for UtpTransport {
    type Item = io::Result<Connection<<Self as Transport>::Connection>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.incoming.is_none() {
            let contex = self.context.clone();
            self.incoming = async move { contex.accept().await }.boxed().into();
        }
        let incoming = self.incoming.as_mut().unwrap().poll_unpin(cx);
        let incoming = incoming.map(|poll| Some(poll.map(|(socket, _)| UtpStream::from(socket))));
        if let Some(conn) = into_connection(incoming, false) {
            self.incoming = None;
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
    poll: Poll<Option<io::Result<UtpStream>>>,
    is_initiator: bool,
) -> Option<io::Result<Connection<UtpStream>>> {
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
