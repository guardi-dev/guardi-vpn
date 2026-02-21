pub mod network;
pub mod interceptor;
pub mod tunnel;

use std::env;
use std::time::Duration;

// TYPES (Согласно ACM.txt)
pub type PeerId = String;
pub type Latency = u32;
pub type Hostname = String;
pub type RouteScore = u32;
pub type Packet = Vec<u8>;
pub type PeerList = Vec<PeerId>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_name = env::var("NODE_NAME").unwrap_or_else(|_| "node_default".into());
    println!("🚀 Guardi-VPN: [{}] в сети.", node_name);

    // 1. Discovery
    let mut peer_rx = network::discovery(node_name.clone()).await;

    // 2. Network Topology Logic
    tokio::spawn(async move {
        while let Some(peer_id) = peer_rx.recv().await {
            let lat = network::ping(peer_id.clone()).await;
            let final_score = network::score(lat, 30);
            println!("📡 Пир: {} | RTT: {}ms | Score: {}", peer_id, lat, final_score);
        }
    });

    // 3. Interceptor Logic (С обработкой ошибок Result)
    tokio::spawn(async move {
        loop {
            // Пытаемся захватить пакет. 
            // sniffer возвращает Result<Packet, String>
            match interceptor::sniffer(443) {
                Ok(pkt) => {
                    let target = interceptor::parser(pkt);
                    // Если Hostname это String, это сработает. 
                    // Если это кастомный тип, используем target.to_string()
                    let str = target.unwrap_or_default();
                    if str.len() > 0 {
                        println!("🔍 Перехвачен запрос к: {}", str);
                    }
                }
                Err(e) => eprintln!("⚠️ Ошибка сниффера: {}", e),
            }
            
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}