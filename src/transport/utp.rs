use async_compat::Compat;
use async_trait::async_trait;
use futures_lite::StreamExt;
use futures_lite::{AsyncRead, AsyncWrite};
use libutp_rs::{UtpContext, UtpSocket};
use std::fmt;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::{Incoming, Transport};

#[derive(Clone)]
pub struct UtpTransport {
    context: Option<UtpContext>,
}

impl fmt::Debug for UtpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UtpTransport").finish()
    }
}

#[async_trait]
impl Transport for UtpTransport {
    type Connection = UtpStream;
    fn new() -> Self {
        Self { context: None }
    }
    async fn listen<A>(&mut self, local_addr: A) -> io::Result<Incoming<Self::Connection>>
    where
        A: ToSocketAddrs + Send,
    {
        if self.context.is_some() {
            panic!("may not listen more than once");
        }
        let addr = local_addr.to_socket_addrs()?.next().unwrap();
        let context = UtpContext::bind(addr)?;
        let listener = context.listener();
        self.context = Some(context);
        let listener = listener.map(|s| s.map(|s| UtpStream::new(s)));
        let incoming = Incoming::new(listener, addr);
        Ok(incoming)
    }
    async fn connect<A>(&mut self, peer_addr: A) -> io::Result<Self::Connection>
    where
        A: ToSocketAddrs + Send,
    {
        if self.context.is_none() {
            panic!("socket is not bound! use listen() first");
        }
        let addr = peer_addr.to_socket_addrs()?.next().unwrap();
        let stream = self.context.as_ref().unwrap().connect(addr).await?;
        let stream = UtpStream::new(stream);
        Ok(stream)
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
