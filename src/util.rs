use blake2_rfc::blake2b::Blake2b;
use std::convert::TryInto;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

pub fn to_socket_addr<A: ToSocketAddrs>(addr: A) -> io::Result<SocketAddr> {
    addr.to_socket_addrs()?.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Invalid socket address",
    ))
}

pub fn hash_topic(namespace: &[u8], topic: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::with_key(32, topic);
    hasher.update(namespace);
    hasher.finalize().as_bytes().try_into().unwrap()
}
