pub mod network;
pub mod interceptor;
pub mod tunnel;

// TYPES from ACM.md
pub type PeerId = String;
pub type Latency = u32;
pub type Hostname = String;
pub type RouteScore = u32;
pub type Packet = Vec<u8>;
pub type PeerList = Vec<PeerId>;

use tokio::task;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Guardi-VPN initialized.");

    // Create a pipe for Packets: Capacity 100 to prevent memory overflow
    let (tx, mut rx) = mpsc::channel::<Packet>(100);

    // L: Task 1 - Intercept (The loop you see in your console)
    task::spawn(async move {
        loop {
            if let Ok(packet) = interceptor::sniffer::capture_packet(3000) {
                let _ = tx.send(packet).await;
            }
            // Small sleep just to prevent CPU melting during mock testing
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });

    // L: Task 2 - Process & Tunnel (The "Brain")
    task::spawn(async move {
        while let Some(packet) = rx.recv().await {
            // 1. Extract Hostname
            if let Some(host) = interceptor::parser::extract_hostname(packet) {
                println!("Logic: Captured request for domain: {}", host);

                // 2. Mocking the Network logic (Ping -> Score -> Select)
                let selected_peer = "node-alpha-5".to_string();
                
                println!("Action: Routing {} through peer {}", host, selected_peer);
            }
        }
    });

    // Wait for Ctrl+C to shut down
    tokio::signal::ctrl_c().await?;
    println!("\nGuardi-VPN gracefully stopped.");
    Ok(())
}