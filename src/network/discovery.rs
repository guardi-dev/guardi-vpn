use crate::PeerId;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::time::Duration;
use std::net::SocketAddr;

pub async fn start(hostname: String) -> mpsc::Receiver<PeerId> {
    // Явный тип PeerId через турбо-рыбу согласно твоим требованиям
    let (tx, rx) = mpsc::channel::<PeerId>(100);
    
    // Создаем сокет для обеих задач (анонс и прослушка)
    // Порт 9999 зафиксирован в нашей логике Discovery
    let socket = UdpSocket::bind("0.0.0.0:9999").await.expect("Discovery: Failed to bind UDP");
    socket.set_broadcast(true).expect("Discovery: Failed to set broadcast");
    
    let arc_socket = std::sync::Arc::new(socket);

    // --- Юнит: Beacon (Active Announcing) ---
    let beacon_socket = arc_socket.clone();
    let beacon_name = hostname.clone();
    tokio::spawn(async move {
        let broadcast_addr: SocketAddr = "255.255.255.255:9999".parse().unwrap();
        let msg = format!("IAM:{}", beacon_name);
        
        loop {
            let _ = beacon_socket.send_to(msg.as_bytes(), broadcast_addr).await;
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    // --- Юнит: Scanner (Passive Listening) ---
    let scanner_socket = arc_socket.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            if let Ok((len, addr)) = scanner_socket.recv_from(&mut buf).await {
                let msg = String::from_utf8_lossy(&buf[..len]);
                if msg.starts_with("IAM:") {
                    // Извлекаем IP как PeerId
                    let found_peer = addr.ip().to_string();
                    let _ = tx.send(found_peer).await;
                }
            }
        }
    });

    rx
}