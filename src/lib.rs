//! Hyperswarm is a peer to peer networking stack for connecting peers.
//!
//! See [Hyperswarm] for how to get started.
//!
//! # Examples
//!
//! ```
//! // tbi
//! ```

#![forbid(unsafe_code, future_incompatible, rust_2018_idioms)]
#![deny(missing_debug_implementations, nonstandard_style)]
// #![warn(missing_docs, missing_doc_code_examples, unreachable_pub)]

mod bootstrap;
mod config;
mod swarm;
mod util;

pub mod discovery;
pub mod transport;

pub use bootstrap::{run_bootstrap_node, BootstrapNode};
pub use config::{Config, TopicConfig};
pub use swarm::Hyperswarm;

pub use discovery::dht::DhtConfig;
pub use discovery::mdns::MdnsConfig;

pub use hyperswarm_dht::{IdBytes, DEFAULT_BOOTSTRAP};
pub use util::hash_topic;

use transport::combined::CombinedStream;
pub use transport::Connection;
pub type HyperswarmStream = Connection<CombinedStream>;
