use std::{error::Error, sync::Arc, process::exit};
use guardi_vpn::network::p2p::P2PEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    
    let p2p = Arc::new(P2PEngine::new());
    let p2p_c = p2p.clone();
    let p2p_handle = tokio::spawn(async move {
        let _ = p2p_c.listen().await;
    });

    let mut tx = p2p.broadcast.subscribe();

    loop {
        if p2p_handle.is_finished() {
            println!("P2P Engine is down, exit!");
            exit(1);
        }
        tokio::select! {
            event = tx.recv() => {
                println!("P2P Event {:?}", event);
            }
        }
    }
}