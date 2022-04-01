use anyhow::Context;
use async_std::task;
use clap::Parser;
use log::*;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

use hyperswarm::{BootstrapNode, DhtConfig};

/// Run a Hyperswarm bootstrap node.
#[derive(Parser, Debug)]
struct Options {
    /// Local address to bind bootstrap node on.
    #[clap(short, long, default_value = "0.0.0.0:49737")]
    address: SocketAddr,

    /// Bootstrap addresses
    #[clap(short, long)]
    bootstrap: Vec<String>,
}

fn main() -> io::Result<()> {
    env_logger::init();
    match task::block_on(async_main()) {
        Err(e) => {
            debug!("{:?}", e);
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        Ok(()) => Ok(()),
    }
}

async fn async_main() -> anyhow::Result<()> {
    let opts: Options = Options::parse();

    let config = DhtConfig::default()
        .set_bootstrap_nodes(&opts.bootstrap[..])
        .set_ephemeral(false);

    let mut config = if opts.bootstrap.len() > 0 {
        let mut addrs = vec![];
        for addr in opts.bootstrap.iter() {
            for addr in addr
                .to_socket_addrs()
                .with_context(|| format!("Invalid socket address \"{}\"", addr))?
            {
                if addr.is_ipv4() {
                    addrs.push(addr);
                }
            }
        }
        config.set_bootstrap_nodes(&addrs[..])
    } else {
        config
    };

    let bootstrap_addrs: Vec<String> = config
        .bootstrap_nodes()
        .map(|v| v.clone().iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| {
            hyperswarm::DEFAULT_BOOTSTRAP
                .to_vec()
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let node = BootstrapNode::new(config, Some(opts.address));
    let (addr, task) = node.run().await?;

    println!("DHT listening on {}", addr);
    debug!(
        "Bootstrapping DHT via nodes: {}",
        bootstrap_addrs.join(", ")
    );

    task.await?;
    Ok(())
}
