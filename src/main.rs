pub mod network;
pub mod interceptor;
pub mod tunnel;

// Типы из твоего блока TYPES
pub type PeerId = String;
pub type Latency = u32;
pub type Hostname = String;
pub type RouteScore = u32;
pub type Packet = Vec<u8>;

use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Guardi-VPN: Система запущена.");

    // Создаем канал для передачи пакетов от снайфера к обработчику
    let (tx, mut rx) = mpsc::channel::<Packet>(100);

    // Ветка перехвата (Interceptor)
    tokio::spawn(async move {
        loop {
            // Имитируем захват пакета
            if let Ok(packet) = interceptor::sniffer::capture_packet(3000) {
                let _ = tx.send(packet).await;
            }
            // Пауза, чтобы не заспамить консоль
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    // Ветка обработки и туннелирования
    tokio::spawn(async move {
        while let Some(packet) = rx.recv().await {
            // 1. Парсим пакет, достаем хост
            if let Some(host) = interceptor::parser::extract_hostname(packet) {
                println!(">>> Перехвачен запрос к: {}", host);

                // 2. Логика сети: расчет лучшего узла (имитация)
                let score = network::score::calculate(50, 20);
                println!("--- Рассчитанный вес маршрута: {}", score);

                let best_peer = "germany-node-01".to_string();
                println!("=== Направляем трафик через: {}", best_peer);
            }
        }
    });

    // Ждем прерывания (Ctrl+C)
    tokio::signal::ctrl_c().await?;
    println!("Guardi-VPN: Завершение работы.");
    Ok(())
}