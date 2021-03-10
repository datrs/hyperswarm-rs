use async_std::stream::StreamExt;
use async_trait::async_trait;
use futures_lite::{AsyncRead, AsyncWrite, FutureExt};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::tcp::{TcpStream, TcpTransport};
use super::utp::{UtpStream, UtpTransport};
use super::{Incoming, Transport};

#[derive(Clone, Debug)]
pub struct CombinedTransport {
    tcp: TcpTransport,
    utp: UtpTransport,
}

#[async_trait]
impl Transport for CombinedTransport {
    type Connection = CombinedStream;
    fn new() -> Self {
        Self {
            tcp: TcpTransport::new(),
            utp: UtpTransport::new(),
        }
    }
    async fn listen<A>(&mut self, local_addr: A) -> io::Result<Incoming<Self::Connection>>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = local_addr.to_socket_addrs()?.next().unwrap();

        let tcp_incoming = self.tcp.listen(addr).await?;
        let addr = tcp_incoming.local_addr();
        let tcp_incoming = tcp_incoming.map(|s| s.map(CombinedStream::Tcp));

        let utp_incoming = self.utp.listen(addr).await?;
        let utp_incoming = utp_incoming.map(|s| s.map(CombinedStream::Utp));

        let combined = tcp_incoming.merge(utp_incoming);
        let incoming = Incoming::new(combined, addr);

        // let incoming = Incoming::new(utp_incoming, addr);

        Ok(incoming)
    }
    async fn connect<A>(&mut self, peer_addr: A) -> io::Result<Self::Connection>
    where
        A: ToSocketAddrs + Send,
    {
        let addr = peer_addr.to_socket_addrs()?.next().unwrap();
        let utp = &mut self.utp;
        let tcp = &mut self.tcp;
        let utp_fut = async {
            let res = utp.connect(addr).await;
            res.map(CombinedStream::Utp)
        };
        // utp_fut.await
        let tcp_fut = async {
            let res = tcp.connect(addr).await;
            res.map(CombinedStream::Tcp)
        };
        utp_fut.race(tcp_fut).await
    }
}

#[derive(Debug, Clone)]
pub enum CombinedStream {
    Tcp(TcpStream),
    Utp(UtpStream),
}

impl CombinedStream {
    pub fn peer_addr(&self) -> SocketAddr {
        match self {
            Self::Tcp(stream) => stream.peer_addr().unwrap(),
            Self::Utp(stream) => stream.peer_addr(),
        }
    }

    pub fn protocol(&self) -> String {
        match self {
            CombinedStream::Tcp(_) => "tcp".into(),
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
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_flush(cx),
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            CombinedStream::Tcp(ref mut stream) => Pin::new(stream).poll_close(cx),
            CombinedStream::Utp(ref mut stream) => Pin::new(stream).poll_close(cx),
        }
    }
}
