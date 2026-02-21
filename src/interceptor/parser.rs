use crate::{Packet, Hostname};
// In a real implementation, we would use crates like 'dns-parser' or 'tls-parser'

/// L: extract domain name from packet header
pub fn extract_hostname(packet: Packet) -> Hostname {
    if packet.len() < 34 {
        return "Invalid".to_string();
    }

    if packet.len() < 34 { return "Small Packet".to_string(); }

    // Пропускаем Ethernet заголовок (14 байт)
    // Байты 26-29: Source IP
    let src = format!("{}.{}.{}.{}", packet[26], packet[27], packet[28], packet[29]);
    // Байты 30-33: Destination IP
    let dst = format!("{}.{}.{}.{}", packet[30], packet[31], packet[32], packet[33]);

    format!("{} -> {}", src, dst)
}