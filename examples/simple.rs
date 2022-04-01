use async_std::prelude::*;
use async_std::stream::StreamExt;
use async_std::task::{self, JoinHandle};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

const DEFAULT_BOOTSTRAP: &str = "localhost:6060";

use hyperswarm::{BootstrapNode, Config, Hyperswarm, HyperswarmStream, TopicConfig};

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let bs_addr = std::env::var("BOOTSTRAP");
    let bs_addr = match bs_addr {
        Err(_) => {
            let (addr, _task) = BootstrapNode::with_addr(DEFAULT_BOOTSTRAP)?.run().await?;
            eprintln!("running bootstrap node on {}", addr);
            addr.to_string()
        }
        Ok(addr) => {
            if &addr == "default" {
                DEFAULT_BOOTSTRAP.to_string()
            } else {
                addr
            }
        }
    };
    eprintln!("using bootstrap address: {}", bs_addr);
    let name = std::env::var("NAME").unwrap_or("rust".to_string());
    let count: u32 = std::env::var("COUNT").unwrap_or("2".to_string()).parse()?;
    let bs_addr: SocketAddr = bs_addr.to_socket_addrs().unwrap().next().unwrap();

    let mut tasks = vec![];

    for i in 0..count {
        let config = Config::default().set_bootstrap_nodes(&[bs_addr]);
        let task = run_node(config, format!("{}.{}", name, i).to_string()).await?;
        eprintln!("running node {}.{}", name, i);
        tasks.push(task);
    }

    for task in tasks {
        task.await;
    }

    Ok(())
}

async fn run_node(config: Config, name: String) -> io::Result<JoinHandle<()>> {
    let mut swarm = Hyperswarm::bind(config).await?;

    let handle = swarm.handle();

    let task = task::spawn(async move {
        while let Some(stream) = swarm.next().await {
            let stream = stream.unwrap();
            on_connection(stream, name.clone());
        }
    });

    let topic = [0u8; 32];
    handle.configure(topic, TopicConfig::both());
    Ok(task)
}

fn on_connection(mut stream: HyperswarmStream, local_name: String) {
    let label = format!(
        "[{}] <{}://{}>",
        local_name,
        stream.protocol(),
        stream.peer_addr()
    );
    eprintln!("{} connect", label);
    task::spawn(async move {
        stream
            .write_all(format!("hi from {}", local_name).as_bytes())
            .await
            .unwrap();
        let mut buf = vec![0u8; 100];
        loop {
            match stream.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let text = String::from_utf8(buf[..n].to_vec()).unwrap();
                    eprintln!("{} message: {}", label, text);
                }
                Ok(_) => {
                    eprintln!("{} close", label);
                    break;
                }
                Err(e) => {
                    eprintln!("{} error: {}", label, e);
                    break;
                }
            }
        }
    });
}
