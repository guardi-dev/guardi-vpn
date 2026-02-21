use std::collections::HashMap;
use std::net::{UdpSocket, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration};
use byteorder::{ByteOrder, BigEndian};
use std::thread;
use uuid::Uuid;
use strum_macros::{EnumString, Display};

/// TYPES (из твоей спецификации)
pub type PeerId = Uuid;

const PROJECT_ID: &str = "GUARDI_VPN";

const STUN_SERVER: &str = "74.125.200.127:19302"; 

#[derive(Debug, EnumString, Display)]
enum Topic {
    #[strum(serialize = "GOSSIP_ALIVE")]
    GossipAlive,

    #[strum(serialize = "QUERY_HOSTS")]
    QueryHosts,

    #[strum(serialize = "HOSTS_REPORT")]
    HostsReport
}

pub struct Node {
    pub id: PeerId,
    pub local_addr: SocketAddr,
    pub public_addr: Option<SocketAddr>,
    pub secret_key: String, // Для идентификации "своих" в сети
    pub target_hosts: Vec<String>, 
    // Карта: PeerId -> { "google.com": true, "internal.corp": false }
    pub network_availability: HashMap<String, HashMap<String, bool>>,
    // Реестр адресов пиров (PeerId -> SocketAddr)
    pub peer_registry: HashMap<String, SocketAddr>,
}

impl Node {
    pub fn new(target_hosts: Vec<String>) -> Self {
        let auto_id = Uuid::new_v4();
        // Ключ проекта, чтобы ноды из разных твоих сборок не перемешались
        Self {
            id: auto_id,
            local_addr: "0.0.0.0:4242".parse().unwrap(),
            public_addr: None,
            target_hosts,
            network_availability: HashMap::new(),
            peer_registry: HashMap::new(),
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
                let msg = format!("{}:{}:{}", secret_cloned, Topic::GossipAlive, id_cloned);
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

                // Фильтр по секретному ключу
                if parts.len() < 2 || parts[0] != self.secret_key { continue; }

                if let Ok(topic) = Topic::from_str(parts[1]) {
                    match topic {
                        Topic::GossipAlive => {
                            let peer_id = parts[2].to_string();
                            println!("Peer ID Discovered: {}", peer_id);

                            if peer_id != self.id.to_string() {

                                // Сохраняем адрес пира
                                self.peer_registry.insert(peer_id.clone(), src);
                                
                                // СРАЗУ запрашиваем список хостов у этого пира
                                self.sync_hosts(&socket, &peer_id);
                            }
                        },
                        Topic::QueryHosts => {
                            self.handle_host_query(&socket, src, parts[2]);
                        },
                        Topic::HostsReport => {
                            if let Some(peer_id) = self.get_peer_id_by_addr(src) {
                                self.handle_host_report(peer_id, parts[2]);
                            }
                        }
                    }
                }
            }
        }
    }

    fn get_peer_id_by_addr(&self, addr: SocketAddr) -> Option<String> {
        self.peer_registry
            .iter()
            .find(|&(_, &val)| val == addr)
            .map(|(key, _)| key.clone())
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

    /// 1. Отправка списка хостов пиру (вызывается сразу после Discovery)
    pub fn sync_hosts(&self, socket: &UdpSocket, peer_id: &str) {
        if let Some(addr) = self.peer_registry.get(peer_id) {
            let hosts_str = self.target_hosts.join(",");
            // Протокол: SECRET:QUERY_HOSTS:host1,host2,host3
            let payload = format!("{}:{}:{}", self.secret_key, Topic::QueryHosts, hosts_str);
            
            let _ = socket.send_to(payload.as_bytes(), addr);
            println!("[Node {}] Sent host query to {}", self.id, peer_id);
        }
    }

    /// 2. Обработка входящего запроса (вызывается в основном loop)
    /// Принимает список от другого пира, проверяет их и шлет ответ
    pub fn handle_host_query(&self, socket: &UdpSocket, src: SocketAddr, hosts_raw: &str) {
        let requested_hosts: Vec<&str> = hosts_raw.split(',').collect();
        let mut results = Vec::new();

        for host in requested_hosts {
            // Здесь будет логика проверки (например, ping или проверка таблицы маршрутов)
            // Пока имитируем, что мы можем всё (true)
            let is_available = true; 
            results.push(format!("{}:{}", host, is_available));
        }

        // Протокол: SECRET:HOSTS_REPORT:host1:true,host2:false
        let response_payload = format!("{}:{}:{}", self.secret_key, Topic::HostsReport, results.join(","));
        let _ = socket.send_to(response_payload.as_bytes(), src);
    }

    /// 3. Обработка отчета (сохранение результата в память ноды)
    pub fn handle_host_report(&mut self, peer_id: String, report_raw: &str) {
        let mut peer_map = HashMap::new();
        
        for entry in report_raw.split(',') {
            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() == 2 {
                let host = parts[0].to_string();
                let available = parts[1] == "true";
                peer_map.insert(host, available);
            }
        }

        println!("[Node {}] Updated availability for peer {}", self.id, peer_id);
        self.network_availability.insert(peer_id, peer_map);
    }
}