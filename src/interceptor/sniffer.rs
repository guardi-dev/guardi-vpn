use pcap::Capture;

pub fn sniffer(port: u32) -> Vec<u8> {
    // 1. Открываем eth0 напрямую (в Docker это стандарт)
    // Мы используем .open(), чтобы начать слушать
    let mut cap = match Capture::from_device("eth0") {
        Ok(c) => c.immediate_mode(true).timeout(100).open().unwrap(),
        Err(_) => return vec![], // Если eth0 нет, возвращаем пустоту
    };

    // 2. Устанавливаем фильтр, чтобы не ловить всё подряд
    let filter = format!("tcp port {}", port);
    cap.filter(&filter, true).ok();

    // 3. Пытаемся захватить РЕАЛЬНЫЙ пакет
    match cap.next_packet() {
        Ok(packet) => {
            packet.to_vec()
        }
        Err(_) => vec![], // Если пакетов нет, возвращаем пустой вектор
    }
}