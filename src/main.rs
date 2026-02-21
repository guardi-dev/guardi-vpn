pub mod network;
pub mod interceptor;
pub mod tunnel;

use std::time::Duration;
use std::fs;
use std::sync::Arc;
use std::collections::HashSet;

use crate::interceptor::parser;

// TYPES (Согласно ACM.txt)
pub type PeerId = String;
pub type Latency = u32;
pub type Hostname = String;
pub type RouteScore = u32;
pub type Packet = Vec<u8>;
pub type PeerList = Vec<PeerId>;

fn get_local_ip() -> String {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("Couldn't bind");
    socket.connect("8.8.8.8:80").expect("Couldn't connect");
    socket.local_addr().expect("Couldn't get local addr").ip().to_string()
}

fn get_node_id() -> String {
    // Читаем MAC-адрес интерфейса eth0 (стандарт для Linux/Docker)
    let mac = fs::read_to_string("/sys/class/net/eth0/address")
        .unwrap_or_else(|_| "000000000000".to_string())
        .trim()
        .replace(":", "");

    // Вместо сложного хеширования просто берем часть MAC и добавляем префикс
    format!("node-{}", &mac[8..]) 
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_name: Arc<String> = Arc::new(get_node_id());
    println!("🚀 Guardi-VPN: [{}] в сети.", node_name);

    let my_ip = get_local_ip();
    println!("🚀 Мой IP определен как: {}", my_ip);

    // 1. Discovery
    let mut peer_rx = network::discovery(node_name.to_string()).await;
    let mut known_peers: HashSet<String> = HashSet::new(); // Коллекция для уникальных IP
    // 2. Network Topology Logic
    tokio::spawn(async move {
        while let Some(peer_id) = peer_rx.recv().await {
            if !known_peers.contains(&peer_id) {
                let lat = network::ping(peer_id.clone()).await;
                let final_score = network::score(lat, 30);
                println!("📡 Пир: {} | RTT: {}ms | Score: {}", peer_id, lat, final_score);
                known_peers.insert(peer_id);
            }
        }
    });

    // 3. Interceptor Logic (С обработкой ошибок Result)
    tokio::spawn(async move {
        loop {
            // Пытаемся захватить пакет. 
            // sniffer возвращает Result<Packet, String>
            let data = interceptor::sniffer(443);
            if data.len() > 0 {
                let parsed_ip = parser(data);
                if parsed_ip == my_ip {
                    continue;
                }
                println!("🔍 Перехвачен запрос к: {}", parsed_ip);
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}