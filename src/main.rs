use std::{sync::Arc, thread};
use guardi_vpn::{network::p2p::P2PEngine, terminal::user_input::App};

#[tokio::main]
async fn main() {
    // === P2P ===
    let p2p = Arc::new(P2PEngine::new());
    let p2p_thread = p2p.clone();
    
    thread::spawn(async move || {
        p2p_thread.listen().await.unwrap();
    });

    // === TERMINAL ===
    App::default(&p2p.broadcast).await;
}