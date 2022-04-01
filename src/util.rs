use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

pub fn to_socket_addr<A: ToSocketAddrs>(addr: A) -> io::Result<SocketAddr> {
    addr.to_socket_addrs()?.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Invalid socket address",
    ))
}
