use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::stream::StreamExt;
use async_std::task;
// use log::*;
// use std::convert::TryFrom;
// use std::net::SocketAddr;

use hyperswarm::{bootstrap_dht, Hyperswarm, IdBytes, JoinOpts, SwarmEvent};
use hyperswarm_dht::DhtConfig;

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let bs_addr = "localhost:6060";
    let bs_addr = bootstrap_dht(Some(bs_addr)).await?;

    let config1 = DhtConfig::default().set_bootstrap_nodes(&[bs_addr]);
    let config2 = DhtConfig::default().set_bootstrap_nodes(&[bs_addr]);

    let mut swarm1 = Hyperswarm::with_config(config1).await?;
    let mut swarm2 = Hyperswarm::with_config(config2).await?;

    let cmd1 = swarm1.commands();
    let cmd2 = swarm2.commands();

    let task1 = task::spawn(async move {
        while let Some(event) = swarm1.next().await {
            match event {
                SwarmEvent::Connection(stream) => on_connection(stream, "rust1".into()),
                _ => {}
            }
        }
    });

    let task2 = task::spawn(async move {
        while let Some(event) = swarm2.next().await {
            match event {
                SwarmEvent::Connection(stream) => on_connection(stream, "rust2".into()),
                _ => {}
            }
        }
    });

    let topic = IdBytes::from([0u8; 32]);
    let opts = JoinOpts {
        announce: true,
        lookup: true,
    };

    cmd1.join(topic.clone(), opts.clone());
    cmd2.join(topic.clone(), opts.clone());

    task1.await;
    task2.await;

    Ok(())
}

fn on_connection(mut stream: TcpStream, local_name: String) {
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
                    eprintln!("[{}] read: {}", local_name, text);
                }
                Ok(_) => {
                    eprintln!("[{}]: connection closed", local_name);
                    break;
                }
                Err(e) => {
                    eprintln!("[{}]: error: {}", local_name, e);
                    break;
                }
            }
        }
    });
}

// async fn timeout(ms: u64) {
//     let _ = async_std::future::timeout(
//         std::time::Duration::from_millis(ms),
//         futures::future::pending::<()>(),
//     )
//     .await;
// }
