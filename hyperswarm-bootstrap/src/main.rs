use async_std::task;
use clap::Clap;
use std::io;
use std::net::SocketAddr;

use hyperswarm::run_bootstrap_node;

/// Options for the storage daemon
#[derive(Clap, Debug)]
struct Options {
    /// Local address to bind bootstrap node on.
    #[clap(short, long)]
    address: Option<SocketAddr>,

    /// Bootstrap addresses
    #[clap(short, long)]
    bootstrap: Vec<SocketAddr>,
}

fn main() -> io::Result<()> {
    env_logger::init();
    task::block_on(async_main())
}

async fn async_main() -> io::Result<()> {
    let opts: Options = Options::parse();
    let (addr, task) = run_bootstrap_node(opts.address).await?;
    println!("listening on {}", addr);
    task.await?;
    Ok(())
}
