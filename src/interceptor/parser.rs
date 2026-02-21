use crate::{Packet, Hostname};
// In a real implementation, we would use crates like 'dns-parser' or 'tls-parser'

/// L: extract domain name from packet header
pub fn extract_hostname(packet: Packet) -> Hostname {

    if packet.len() < 40 { return "Control".to_string(); }

    let dst = format!("{}.{}.{}.{}", packet[30], packet[31], packet[32], packet[33]);
    
    // Превращаем хвост пакета (полезную нагрузку) в строку, 
    // игнорируя невалидные символы.
    let payload = String::from_utf8_lossy(&packet[54..]);

    // Если это TLS Client Hello, там часто можно встретить имя хоста.
    // Это упрощенный поиск для обучения:
    if let Some(pos) = payload.find("com") { // Ищем расширения вроде .com, .org, .net
        // Вырезаем кусок текста вокруг найденного расширения
        let start = pos.saturating_sub(10);
        let end = (pos + 3).min(payload.len());
        return format!("{} (Host: {}...)", dst, &payload[start..end].trim());
    }

    dst
}