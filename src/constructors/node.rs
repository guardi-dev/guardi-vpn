use std::net::{UdpSocket, Ipv4Addr, SocketAddr};
use std::time::{Duration};
use byteorder::{ByteOrder, BigEndian};
use std::thread;
use uuid::Uuid;

/// TYPES (из твоей спецификации)
pub type PeerId = Uuid;

const PROJECT_ID: &str = "GUARDI_VPN";

const STUN_SERVER: &str = "74.125.200.127:19302"; 


pub struct Node {
    pub id: PeerId,
    pub local_addr: SocketAddr,
    pub public_addr: Option<SocketAddr>,
    pub secret_key: String, // Для идентификации "своих" в сети
}

impl Node {
    pub fn new() -> Self {
        let auto_id = Uuid::new_v4();
        // Ключ проекта, чтобы ноды из разных твоих сборок не перемешались
        Self {
            id: auto_id,
            local_addr: "0.0.0.0:4242".parse().unwrap(),
            public_addr: None,
            secret_key: PROJECT_ID.to_string(),
        }
    }

    /// Основной метод: запускает обнаружение и делает ноду видимой
    pub fn start(&mut self) {
        // 1. Создаем сокет, который будет использоваться и для STUN, и для Discovery
        let socket = UdpSocket::bind(self.local_addr).expect("Failed to bind socket");
        socket.set_broadcast(true).expect("Failed to set broadcast");
        
        println!("Node [{}] started on {}", self.id, self.local_addr);

        // 2. Получаем внешний IP/Port через STUN (Hole Punching)
        if let Ok(addr) = self.get_stun_endpoint(&socket) {
            self.public_addr = Some(addr);
            println!("Public Identity: {:?}", addr);
        }

        let socket_shared = socket.try_clone().expect("Failed to clone socket");
        let id_cloned = self.id.clone();
        let secret_cloned = self.secret_key.clone();

        // 3. Thread: Broadcast Discovery (Gossip)
        // Нода кричит в сеть: "Я тут, мой PeerId такой-то"
        thread::spawn(move || {
            let broadcast_addr: SocketAddr = "255.255.255.255:4242".parse().unwrap();
            loop {
                let msg = format!("{}:{}", secret_cloned, id_cloned);
                let _ = socket_shared.send_to(msg.as_bytes(), broadcast_addr);
                thread::sleep(Duration::from_secs(5));
            }
        });

        // 4. Thread: Listener
        // Слушаем входящие анонсы от других нод того же проекта
        let mut buf = [0u8; 1024];
        loop {
            if let Ok((size, src)) = socket.recv_from(&mut buf) {
                let raw_msg = String::from_utf8_lossy(&buf[..size]);
                let parts: Vec<&str> = raw_msg.split(':').collect();
                
                if parts.len() == 2 && parts[0] == self.secret_key {
                    let discovered_peer_id = parts[1];
                    if discovered_peer_id != self.id.to_string() {
                        println!("Discovered Peer: {} at {}", discovered_peer_id, src);
                        // Здесь логика передачи в PeerList/Discovery
                    }
                }
            }
        }
    }

    fn get_stun_endpoint(&self, socket: &UdpSocket) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;

        let mut packet = [0u8; 20];
        BigEndian::write_u16(&mut packet[0..2], 0x0001);
        BigEndian::write_u32(&mut packet[4..8], 0x2112A442);
        packet[8..20].copy_from_slice(b"STUN-ID-NODE");

        socket.send_to(&packet, STUN_SERVER)?;
        
        let mut buf = [0u8; 512];
        let (len, _) = socket.recv_from(&mut buf)?;

        let mut pos = 20;
        while pos + 4 <= len {
            let attr_type = BigEndian::read_u16(&buf[pos..pos+2]);
            let attr_len = BigEndian::read_u16(&buf[pos+2..pos+4]) as usize;
            pos += 4;
            if attr_type == 0x0020 {
                let x_port = BigEndian::read_u16(&buf[pos+2..pos+4]);
                let x_ip = BigEndian::read_u32(&buf[pos+4..pos+8]);
                let port = x_port ^ 0x2112;
                let ip = Ipv4Addr::from(x_ip ^ 0x2112A442);
                return Ok(SocketAddr::new(ip.into(), port));
            }
            pos += attr_len;
        }
        Err("STUN ERROR".into())
    }
}