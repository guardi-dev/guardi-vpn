use std::{fs, sync::Arc, thread};
use guardi_vpn::{network::p2p::P2PEngine, terminal::user_input::App};

#[tokio::main]
async fn main() {
    let _ = fs::remove_dir_all("tmp");
    let _ = fs::create_dir("tmp");

    // === P2P ===
    let p2p = Arc::new(P2PEngine::new());
    let p2p_thread = p2p.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = p2p_thread.listen().await;
        })
    });

    // === TERMINAL ===
    App::default(&p2p.broadcast).await;
}