//! Peer to peer networking stack
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

pub mod discovery;
pub mod transport;

pub use bootstrap::run_bootstrap_node;
pub use config::{DhtOptions, TopicConfig};
pub use swarm::Hyperswarm;

use transport::combined::CombinedStream;
pub use transport::Connection;
pub type HyperswarmStream = Connection<CombinedStream>;
