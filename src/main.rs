use std::{sync::Arc};
use guardi_vpn::{network::p2p::P2PEngine, terminal::user_input::App};

#[tokio::main]
async fn main() {
    
    // === P2P ===
    let p2p = Arc::new(P2PEngine::new());
    let p2p_thread = p2p.clone();
    
    tokio::spawn(async move {
        let _ = p2p_thread.listen().await;
    });

    // === TERMINAL ===
    App::default(&p2p.broadcast).await;
}