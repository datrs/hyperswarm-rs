use async_trait::async_trait;
use futures_lite::{AsyncRead, AsyncWrite};
// use log::*;
use std::collections::HashSet;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::tcp::{TcpStream, TcpTransport};
#[cfg(feature = "transport_utp")]
use super::utp::{UtpStream, UtpTransport};
use super::{Connection, Transport};

#[derive(Debug)]
pub struct CombinedTransport {
    tcp: TcpTransport,
    #[cfg(feature = "transport_utp")]
    utp: UtpTransport,
    local_addr: SocketAddr,
    connected: HashSet<SocketAddr>,
}

impl CombinedTransport {
    pub async fn bind<A>(local_addr: A) -> io::Result<Self>
    where
        A: ToSocketAddrs + Send,
    {
        let tcp = TcpTransport::bind(local_addr).await?;
        let local_addr = tcp.local_addr();
        #[cfg(feature = "transport_utp")]
        let utp = UtpTransport::bind(local_addr).await?;
        Ok(Self {
            tcp,
            #[cfg(feature = "transport_utp")]
            utp,
            local_addr,
            connected: HashSet::new(), // pending_connects: HashSet::new(),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    fn on_poll_connection<T, F>(
        &mut self,
        poll: Poll<Option<io::Result<Connection<T>>>>,
        map: F,
    ) -> Option<io::Result<Connection<CombinedStream>>>
    where
        T: std::fmt::Debug + AsyncRead + AsyncWrite + Unpin,
        F: Fn(T) -> CombinedStream,
    {
        match poll {
            Poll::Pending => None,
            Poll::Ready(None) => None,
            Poll::Ready(Some(Err(err))) => Some(Err(err)),
            Poll::Ready(Some(Ok(conn))) => self.on_connection(conn, map),
        }
    }

    fn on_connection<T, F>(
        &mut self,
        conn: Connection<T>,
        map: F,
    ) -> Option<io::Result<Connection<CombinedStream>>>
    where
        T: std::fmt::Debug + AsyncRead + AsyncWrite + Unpin,
        F: Fn(T) -> CombinedStream,
    {
        let (stream, peer_addr, is_initiator, protocol) = conn.into_parts();
        let stream = map(stream);
        let conn = Connection::new(stream, peer_addr, is_initiator, protocol);
        Some(Ok(conn))
        // let addr_without_port = peer_addr.set_port(0);
        // if !self.connected.contains(&peer_addr) {
        //     self.connected.insert(peer_addr.clone());
        //     let stream = map(stream);
        //     let conn = Connection::new(stream, peer_addr, is_initiator, protocol);
        //     Some(Ok(conn))
        // } else {
        //     debug!(
        //         "skip double connection to {} via {} (init {})",
        //         peer_addr, protocol, is_initiator
        //     );
        //     None
        // }
    }
}

#[async_trait]
impl Transport for CombinedTransport {
    type Connection = CombinedStream;
    fn connect(&mut self, peer_addr: SocketAddr) {
        self.tcp.connect(peer_addr);
        #[cfg(feature = "transport_utp")]
        self.utp.connect(peer_addr);
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<io::Result<Connection<Self::Connection>>>> {
        let tcp_next = Pin::new(&mut self.tcp).poll_next(cx);
        if let Some(res) = self.on_poll_connection(tcp_next, CombinedStream::Tcp) {
            return Poll::Ready(Some(res));
        }

        #[cfg(feature = "transport_utp")]
        {
            let utp_next = Pin::new(&mut self.utp).poll_next(cx);
            if let Some(res) = self.on_poll_connection(utp_next, CombinedStream::Utp) {
                return Poll::Ready(Some(res));
            }
        }

        Poll::Pending
    }
}

#[derive(Debug)]
pub enum CombinedStream {
    Tcp(TcpStream),
    #[cfg(feature = "transport_utp")]
    Utp(UtpStream),
}

impl CombinedStream {
    pub fn peer_addr(&self) -> SocketAddr {
        match self {
            Self::Tcp(stream) => stream.peer_addr().unwrap(),
            #[cfg(feature = "transport_utp")]
            Self::Utp(stream) => stream.peer_addr(),
        }
    }

    pub fn protocol(&self) -> String {
        match self {
            CombinedStream::Tcp(_) => "tcp".into(),
            #[cfg(feature = "transport_utp")]
            CombinedStream::Utp(_) => "utp".into(),
        }
    }
}

impl AsyncRead for CombinedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
            #[cfg(feature = "transport_utp")]
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for CombinedStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
            #[cfg(feature = "transport_utp")]
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_flush(cx),
            #[cfg(feature = "transport_utp")]
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_close(cx),
            #[cfg(feature = "transport_utp")]
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_close(cx),
        }
    }
}
