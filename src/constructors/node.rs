use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{UdpSocket, Ipv4Addr, SocketAddr, IpAddr};
use std::str::FromStr;
use std::time::{Duration};
use byteorder::{ByteOrder, BigEndian};
use std::thread;
use uuid::Uuid;
use strum_macros::{EnumString, Display};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::RwLock;

/// TYPES (из твоей спецификации)
pub type PeerId = Uuid;

const PROJECT_ID: &str = "GUARDI_VPN";

const STUN_SERVER: &str = "stun:3478"; // google stun "74.125.200.127:19302"; 

#[derive(Debug, EnumString, Display)]
enum Topic {
    #[strum(serialize = "GOSSIP_ALIVE")]
    GossipAlive,

    #[strum(serialize = "QUERY_HOSTS")]
    QueryHosts,

    #[strum(serialize = "HOSTS_REPORT")]
    HostsReport,

    #[strum(serialize = "REPLAY_TRAFFIC")]
    ReplayTraffic,
}

pub struct Node {
    pub socket: Arc<UdpSocket>,
    pub id: PeerId,
    pub public_addr: Option<SocketAddr>,
    pub secret_key: String, // Для идентификации "своих" в сети
    pub target_hosts: Vec<String>, 
    // Карта: PeerId -> { "google.com": true, "internal.corp": false }
    pub network_availability: HashMap<String, HashMap<String, bool>>,
    // Реестр адресов пиров (PeerId -> SocketAddr)
    pub peer_registry: HashMap<String, SocketAddr>,
}

impl Node {
    pub fn new(target_hosts: Vec<String>) -> Arc<RwLock<Self>> {
        let auto_id = Uuid::new_v4();
        let socket = UdpSocket::bind("0.0.0.0:4242").expect("Couldn't bind");
        socket.set_nonblocking(false).unwrap();
        // Ключ проекта, чтобы ноды из разных твоих сборок не перемешались
        Arc::new(RwLock::new(Self {
            socket: Arc::new(socket),
            id: auto_id,
            public_addr: None,
            target_hosts,
            network_availability: HashMap::new(),
            peer_registry: HashMap::new(),
            secret_key: PROJECT_ID.to_string(),
        }))
    }

    /// Основной метод: запускает обнаружение и делает ноду видимой
    pub fn start(node: &Arc<RwLock<Self>>) {
        // 1. Создаем сокет, который будет использоваться и для STUN, и для Discovery
        let (socket, id, secret_key) = {
            let guard = node.write().unwrap();
            (
                guard.socket.clone(),
                guard.id.clone(),
                guard.secret_key.clone()
            )
        };
        socket.set_broadcast(true).expect("Failed to set broadcast");
        println!("Node [{}] started on {}", id, socket.local_addr().unwrap().to_string());

        // 2. Получаем внешний IP/Port через STUN (Hole Punching)
        let public_addr = Some(Node::get_stun_endpoint(&socket).unwrap());
        println!("Public Identity: {:?}", public_addr);
        {
            node.write().unwrap().public_addr = public_addr;
        };

        // 3. Thread: Broadcast Discovery (Gossip)
        // Нода кричит в сеть: "Я тут, мой PeerId такой-то"
        let socket_cloned = socket.clone();
        let secret_cloned = secret_key.clone();
        thread::spawn(move || {
            loop {
                let msg = format!("{}:{}:{}", &secret_cloned, Topic::GossipAlive, &id);
                let broadcast = "255.255.255.255:4242";
                match socket_cloned.send_to(msg.as_bytes(), broadcast) {
                    Err(e) => {
                        println!("[Error] {}", e);
                    },
                    Ok(_) => {
                    }
                }
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
                if parts.len() < 2 || parts[0] != secret_key { continue; }
                
                if let Ok(topic) = Topic::from_str(parts[1]) {
                    match topic {
                        Topic::GossipAlive => {
                            let peer_id = parts[2].to_string();
                            if peer_id == id.to_string() { continue; }

                            println!("Peer ID Discovered: {}", peer_id);
                            // Сохраняем адрес пира
                            {
                                node.write().unwrap().peer_registry.insert(peer_id.clone(), src.clone());
                            };
                            // СРАЗУ запрашиваем список хостов у этого пира
                            Node::sync_hosts(&node, &socket, &peer_id);
                        },
                        Topic::QueryHosts => {
                            Node::handle_host_query(&node, &socket, src, parts[2]);
                        },
                        Topic::HostsReport => {
                            let peer_id = Node::get_peer_id_by_addr(&node, src).unwrap();
                            if peer_id == id.to_string() { continue; }
                            Node::handle_host_report(&node, peer_id, parts[2]);
                        },
                        Topic::ReplayTraffic => {
                            let data = parts[2].as_bytes().to_vec(); // Данные копируем, т.к. они из временного буфера
                            Node::handle_replay(&node, &socket, src, &data);
                        }
                    }
                }
            }
        }
    }

    fn get_peer_id_by_addr(node: &Arc<RwLock<Self>>, addr: SocketAddr) -> Option<String> {
        node.write().unwrap().peer_registry
            .iter()
            .find(|&(_, &val)| val == addr)
            .map(|(key, _)| key.clone())
    }

    fn get_stun_endpoint(socket: &Arc<UdpSocket>) -> Result<SocketAddr, Box<dyn std::error::Error>> {
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
    pub fn sync_hosts(node: &Arc<RwLock<Node>>, socket: &UdpSocket, peer_id: &str) {
        let (peer_registry, target_hosts, secret_key, id) = {
            let guard = node.write().unwrap();
            (
                guard.peer_registry.clone(), 
                guard.target_hosts.clone(),
                guard.secret_key.clone(),
                guard.id.clone()
            )
        };
        if let Some(addr) = peer_registry.get(peer_id) {
            let hosts_str = target_hosts.join(",");
            // Протокол: SECRET:QUERY_HOSTS:host1,host2,host3
            let payload = format!("{}:{}:{}", secret_key, Topic::QueryHosts, hosts_str);
            
            let _ = socket.send_to(payload.as_bytes(), addr);
            println!("[Node {}] Sent host query to {}", id, peer_id);
        }
    }

    /// 2. Обработка входящего запроса (вызывается в основном loop)
    /// Принимает список от другого пира, проверяет их и шлет ответ
    pub fn handle_host_query(node: &Arc<RwLock<Self>>, socket: &UdpSocket, src: SocketAddr, hosts_raw: &str) {
        let secret_key = {
            let guard = node.write().unwrap();
            guard.secret_key.clone()
        };
        let requested_hosts: Vec<&str> = hosts_raw.split(',').collect();
        let mut results = Vec::new();

        for host in requested_hosts {
            // Здесь будет логика проверки (например, ping или проверка таблицы маршрутов)
            // Пока имитируем, что мы можем всё (true)
            let is_available = true; 
            results.push(format!("{}|{}", host, is_available));
        }

        // Протокол: SECRET:HOSTS_REPORT:host1:true,host2:false
        let response_payload = format!("{}:{}:{}", secret_key, Topic::HostsReport, results.join(","));
        let _ = socket.send_to(response_payload.as_bytes(), src);
    }

    /// 3. Обработка отчета (сохранение результата в память ноды)
    pub fn handle_host_report(node: &Arc<RwLock<Self>>, peer_id: String, report_raw: &str) {
        let mut peer_map = HashMap::new();

        for entry in report_raw.split(',') {
            let parts: Vec<&str> = entry.split('|').collect();
            if parts.len() == 2 {
                let host = parts[0].to_string();
                let available = parts[1] == "true";
                peer_map.insert(host, available);
            }
        }
        let id = {
            node.write().unwrap().id.clone()
        };
        println!("[Node {}] Updated availability for peer {}", id, peer_id);
        {
            node.write().unwrap().network_availability.insert(peer_id, peer_map);
        }
    }

    /// Отправка сырого пакета конкретному пиру
    pub fn send_packet(&self, peer_id: &str, packet: Vec<u8>) {
        if let Some(addr) = self.peer_registry.get(peer_id) {
            // Формат: SECRET:REPLAY_TRAFFIC:[RAW_BYTES]
            // Используем вектор байтов для сборки пакета
            let mut payload = format!("{}:{}:", self.secret_key, Topic::ReplayTraffic)
                .into_bytes();
            payload.extend(packet);

            let _ = self.socket.send_to(&payload, addr);
        }
    }

    pub fn handle_replay(node: &Arc<RwLock<Self>>, socket: &UdpSocket, src: SocketAddr, data: &[u8]) {
        let secret_key = {
            node.write().unwrap().secret_key.clone()
        };
        // 1. Валидация размера (Минимум: 4 байта IP + 2 байта Port)
        if data.len() < 6 { return; }

        // 2. Извлекаем целевой IP и Порт
        let ip = Ipv4Addr::new(data[0], data[1], data[2], data[3]);
        let port = u16::from_be_bytes([data[4], data[5]]);
        let target_addr = SocketAddr::new(IpAddr::V4(ip), port);

        // 3. Фильтр безопасности (только 80 и 443)
        if port != 80 && port != 443 {
            println!("[Security] Blocked traffic to port {}", port);
            return;
        }

        let payload = &data[6..];
        println!("[Replay] Forwarding to {}...", target_addr);

        // 4. Выход в интернет через TCP
        match TcpStream::connect_timeout(&target_addr.into(),Duration::from_secs(5)) {
            // 1. Тайм-аут не сработал (мы успели за 5 секунд)
            Ok(mut stream) => {
                println!("[Replay] Connected to {}. Sending payload...", target_addr);

                // Отправляем данные (асинхронно)
                if let Err(e) = stream.write_all(payload) {
                    eprintln!("[Replay] Failed to write to TCP: {}", e);
                    return;
                }

                // Читаем ответ от сервера
                let mut response = Vec::new();
                // В Tokio вместо read_to_end часто используют чтение в буфер, 
                // но для простоты прочитаем всё до закрытия сокета сервером:
                if let Err(e) = stream.read_to_end(&mut response) {
                    eprintln!("[Replay] Failed to read from TCP: {}", e);
                }

                // Отправляем собранные байты обратно инициатору через UDP
                if !response.is_empty() {
                    Node::send_response_back(&secret_key, socket, src, response);
                }
            }
            // 4. Сработал тайм-аут (сервер молчит)
            Err(_) => {
                eprintln!("[Replay] Timeout: {} did not respond in 5s", target_addr);
            }
        }
    }

    fn send_response_back(secret_key: &String, socket: &UdpSocket, to: SocketAddr, data: Vec<u8>) {
        let mut msg = format!("{}:{}:", secret_key, Topic::ReplayTraffic).into_bytes();
        msg.extend(data);
        let _ = socket.send_to(&msg, to);
    }

    pub fn get_peer_id_by_host (&self, host: &String) -> Option<&String> {
        for (peer_id, val) in self.network_availability.iter() {
            let av = val.get(host).unwrap_or(&false);
            if *av {
                return Some(peer_id);
            }
        }
        return None;
    }
}