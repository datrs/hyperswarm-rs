use async_std::prelude::*;
use async_std::stream::StreamExt;
use async_std::task;
// use std::net::{SocketAddr, ToSocketAddrs};

use hyperswarm::{run_bootstrap_node, Config, Hyperswarm, HyperswarmStream, TopicConfig};

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let bs_addr = "localhost:6060";
    let (bs_addr, bs_task) = run_bootstrap_node(Some(bs_addr)).await?;
    // let bs_addr: SocketAddr = bs_addr.to_socket_addrs().unwrap().next().unwrap();

    let config = Config::default().set_bootstrap_nodes(vec![bs_addr]);

    let mut swarm1 = Hyperswarm::bind(config.clone()).await?;
    let mut swarm2 = Hyperswarm::bind(config).await?;

    let handle1 = swarm1.handle();
    let handle2 = swarm2.handle();

    let task1 = task::spawn(async move {
        while let Some(stream) = swarm1.next().await {
            let stream = stream.unwrap();
            on_connection(stream, "rust1".into());
        }
    });

    let task2 = task::spawn(async move {
        while let Some(stream) = swarm2.next().await {
            let stream = stream.unwrap();
            on_connection(stream, "rust2".into());
        }
    });

    let topic = [0u8; 32];
    handle1.configure(topic, TopicConfig::both());
    handle2.configure(topic, TopicConfig::both());

    task1.await;
    task2.await;
    bs_task.await?;

    Ok(())
}

fn on_connection(mut stream: HyperswarmStream, local_name: String) {
    let label = format!(
        "[{} -> {}://{}]",
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
                    eprintln!("{} read: {}", label, text);
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
