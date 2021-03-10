<h1 align="center">hyperswarm-rs</h1>
<div align="center">
  <strong>
    Peer to peer networking stack
  </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/hyperswarm">
    <img src="https://img.shields.io/crates/v/hyperswarm.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/hyperswarm">
    <img src="https://img.shields.io/crates/d/hyperswarm.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs.rs docs -->
  <a href="https://docs.rs/hyperswarm">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>

<div align="center">
  <h5>
    <a href="https://docs.rs/hyperswarm">
      API Docs
    </a>
    <span> | </span>
    <a href="https://github.com/Frando/hyperswarm/blob/master.github/CONTRIBUTING.md">
      Contributing
    </a>
  </h5>
</div>

*NOTE: This is still in early stages. See the roadmap below. Please feel free to open issues and send PRs :-)*

## Installation
```sh
$ cargo add hyperswarm --git https://github.com/Frando/hyperswarm-rs.git
```

## Usage

Hyperswarm is a networking stack for connecting peers who are interested in a topic. This project is a port of the [Node.js implementation of Hyperswarm](https://github.com/hyperswarm/hyperswarm).

This crate exposes a `Hyperswarm` struct. After binding it, this will:

- Start and bootstrap a local DHT node
- Bind a socket for mDNS discovery
- Announce and lookup any 32 byte topic key over both mDNS and the DHT
- Connect to all peers that are found over both TCP and UTP

It currently depends on the unreleased [hyperswarm-dht](https://github.com/mattsse/hyperswarm-dht) crate and therefore is also not yet released on crates.io.

The API is designed to be very simple:

```rust
use async_std::task;
use futures_lite::{AsyncReadExt, AsyncWriteExt, StreamExt};
use hyperswarm::{Config, Hyperswarm, HyperswarmStream, TopicConfig};
use std::io;

#[async_std::main]
async fn main() -> io::Result<()> {
    // Bind and initialize the swarm with the default config.
    // On the config you can e.g. set bootstrap addresses.
    let config = Config::default();
    let mut swarm = Hyperswarm::bind(config).await?;

    // A topic is any 32 byte array. Usually, this would be the hash of some identifier.
    // Configuring the swarm for a topic starts to lookup and/or announce this topic
    // and connect to peers that are found.
    let topic = [0u8; 32];
    swarm.configure(topic, TopicConfig::announce_and_lookup());

    // The swarm is a Stream of new HyperswarmStream peer connections.
    // The HyperswarmStream is a wrapper around either a TcpStream or a UtpSocket.
    // Usually you'll want to run some loop over the connection, so let's spawn a task
    // for each connection.
    while let Some(stream) = swarm.next().await {
        task::spawn(on_connection(stream?));
    }

    Ok(())
}

// A HyperswarmStream is AsyncRead + AsyncWrite, so you can use it just
// like a TcpStream. Here, we'll send an initial message and then keep
// reading from the stream until it is closed by the remote.
async fn on_connection(mut stream: HyperswarmStream) -> io::Result<()> {
    stream.write_all(b"hello there").await?;
    let mut buf = vec![0u8; 64];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => return Ok(()),
            Err(e) => return Err(e),
            Ok(n) => eprintln!("received: {}", std::str::from_utf8(&buf[..n]).unwrap()),
        }
    }
}
```

See [`examples/simple.rs`](examples/simple.rs) for a working example that also runs a bootstrap node. That example can also find and connect to NodeJS peers. To try it out:

```sh
cargo run --example simple
# in another terminal
node js/simple.js
```

Currently, the DHT bootstrap node has to be run from Rust. The Rust implementation does not find peers on a NodeJS bootstrap node. 

## Roadmap

- [x] Initial implementation
- [ ] Find peers over the Hyperswarm DHT
    - [x] Both NodeJS and Rust peers are found if connecting to a Rust bootstrap node
    - [ ] Fix [hyperswarm-dht](https://github.com/mattsse/hyperswarm-dht) to work with NodeJS bootstrap nodes
- [ ] Find peers over mDNS
    - [ ] Change colmeia-mdns to better fit the architecture here or copy and adapt the mdns code over into the mdns module
- [x] Connect to peers over TCP
- [ ] Connect to peers over UTP
    - [x] Can connect to peers over UTP
    - [ ] Fix issues in [libutp-rs](https://github.com/johsunds/libutp-rs) - sometimes the connection is not flushed properly

## Safety
This crate uses ``#![deny(unsafe_code)]`` to ensure everything is implemented in
100% Safe Rust.

## Contributing
Want to join us? Check out our ["Contributing" guide][contributing] and take a
look at some of these issues:

- [Issues labeled "good first issue"][good-first-issue]
- [Issues labeled "help wanted"][help-wanted]

[contributing]: https://github.com/Frando/hyperswarm/blob/master.github/CONTRIBUTING.md
[good-first-issue]: https://github.com/Frando/hyperswarm/labels/good%20first%20issue
[help-wanted]: https://github.com/Frando/hyperswarm/labels/help%20wanted

## License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
