use async_std::stream::Stream;
use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncWrite};
use std::fmt;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::task::{Context, Poll};

pub mod combined;
pub mod tcp;
pub mod utp;

pub struct Incoming<S> {
    local_addr: SocketAddr,
    stream: Box<dyn Stream<Item = io::Result<S>> + Send + Unpin>,
}

impl<S> fmt::Debug for Incoming<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incoming")
            .field("local_addr", &self.local_addr)
            // .field("stream", &*self.stream)
            .finish()
    }
}

impl<S> Incoming<S>
where
    S: AsyncRead + AsyncWrite + Send + Clone,
{
    pub fn new<L>(listener: L, local_addr: SocketAddr) -> Self
    where
        L: Stream<Item = io::Result<S>> + Send + Unpin + 'static,
    {
        let listener = Box::new(listener);
        Self {
            stream: listener,
            local_addr,
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl<S> Stream for Incoming<S> {
    type Item = io::Result<S>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

#[async_trait]
pub trait Transport: Clone {
    type Connection: AsyncRead + AsyncWrite + Send;
    fn new() -> Self;
    async fn listen<A>(&mut self, local_addr: A) -> io::Result<Incoming<Self::Connection>>
    where
        A: ToSocketAddrs + Send;
    async fn connect<A>(&mut self, peer_addr: A) -> io::Result<Self::Connection>
    where
        A: ToSocketAddrs + Send;
}

// trait TransportStream: AsyncRead + AsyncWrite + Send + Clone {}

// impl TransportStream for TcpStream {}
// impl TransportStream for super::Connection {}

// enum Transports {
//     Tcp(TcpStream),
//     Utp(super::Connection)
// }

// impl Transports {
//     // fn into_inner(&mut self) -> Box<dyn TransportStream> {
//     //     match self {
//     //         Self::Tcp(stream) => Box::new(stream),
//     //         Self::Utp(stream) => Box::new(stream),
//     //     }
//     // }
//     // fn as_mut(&mut self) -> &mut dyn TransportStream {
//     //     match self {
//     //         Self::Tcp(ref mut stream) => stream,
//     //         Self::Utp(ref mut stream) => stream,
//     //     }
//     // }
// }

// pub async fn demo() {
//     let stream1 = {
//         let stream = TcpStream::connect("localhost:1234").await.unwrap();
//         let stream: Box<dyn DynConnection<Stream = TcpStream>> = Box::new(stream);
//         stream
//     };

//     let stream2 = {
//         let addr = "localhost:1233";
//         // let stream = TcpStream::connect(addr).await.unwrap();
//         // let stream = super::Connection::tcp(stream, true, addr, None);
//         let stream = FakeStream {};
//         let stream: Box<dyn DynConnection<Stream = super::Connection>> = Box::new(stream);
//         stream
//     };
//     let streams: HashMap<&str, Box<dyn DynConnection>> = HashMap::new();
//     streams.insert("stream1", stream1);
//     streams.insert("stream2", stream2);
// }

// pub struct Connection {
//     stream: Box<dyn AsyncRead + AsyncWrite + Send + Clone>
// }
