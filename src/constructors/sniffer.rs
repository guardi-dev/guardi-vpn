use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use std::process::Command;
use dns_lookup::lookup_host;
use std::io::Read;

use crate::constructors::node::Node;

pub struct Sniffer {
    node: Arc<RwLock<Node>>,
    target_hosts: Vec<String>,
    // Мапа обернута для безопасного доступа из разных потоков
    ip_to_peer_id: Arc<RwLock<HashMap<String, String>>>,
}

impl Sniffer {

    pub fn new (node: Arc<RwLock<Node>>, target_hosts: Vec<String>) -> Arc<Self> {
        Arc::new(Self {
            ip_to_peer_id: Arc::new(RwLock::new(HashMap::new())),
            node,
            target_hosts
        })
    }

    fn check_availability (&self) -> Result<(), Box<dyn std::error::Error>> {

        for host in &self.target_hosts {
            // Проверяем в Node: доступен ли пир для этого хоста?
            let peer_id_wrapped = {
                self.node.write().unwrap().get_peer_id_by_host(host).cloned()
            };
            if peer_id_wrapped.is_none() {
                continue;
            }
            let peer_id = peer_id_wrapped.unwrap();
            
            if let Ok(ips) = lookup_host(host) {
                for ip in ips {
                    let ip_str = ip.to_string();
                    println!("IpSTR: {}", ip_str);
                    
                    // Если этого IP еще нет в нашей мапе — добавляем и ставим маршрут
                    let ip_to_peer_id_clone = {
                        self.ip_to_peer_id.write().unwrap().clone()
                    };
                    if !ip_to_peer_id_clone.contains_key(&ip_str) {
                        {
                            self.ip_to_peer_id.write().unwrap().insert(ip_str.clone(), peer_id.clone());
                        };
                        println!("+ Start Write Route: {} -> пир {}", &ip_str, &peer_id);
                        // Прописываем маршрут в ОС
                        Command::new("ip")
                            .args(&["route", "add", &ip_str, "dev", "tun0"])
                            .status()
                            .ok();
                        
                        println!("+ Маршрут: {} -> пир {}", &ip_str, &peer_id);
                    }
                }
            } 
        }

        Ok(())
    }

    pub fn start(self: Arc<Self>) {
        let self_clone = self.clone();

        // --- ПОТОК 1: РЕВИЗОР (раз в 10 секунд) ---
        thread::spawn(move || {
            loop {
                match self_clone.check_availability() {
                    Err(e) => {
                        println!("[CheckAvailabilityError] {}", e);
                    },
                    Ok(_) => {}
                };
                thread::sleep(Duration::from_secs(10));
            }
        });

        // --- ПОТОК 2: ТРАФИК (основной) ---
        let mut config = tun::Configuration::default();
        config
            .tun_name("tun0")
            .address("10.0.0.1")       // Твой внутренний IP туннеля
            .netmask("255.255.255.255") // МАСКА /32 — это КЛЮЧЕВОЙ момент
            .up();
        let mut dev = tun::create(&config).expect("Failed to create TUN");
        let mut buf = [0; 4096];

        println!("[SNIFFER] Запущен. Слушаю TUN...");
        
        loop {
            let n = dev.read(&mut buf).unwrap();
    
            // Смещение для UDP портов в IPv4 — 20 (src) и 22 (dest)
            let dest_port = u16::from_be_bytes([buf[22], buf[23]]);

            // ЕСЛИ ЭТО GOSSIP (4242) — МЫ ЕГО НЕ ДРОПАЕМ
            if dest_port == 4242 {
                // Здесь костыль: если пакет попал в TUN, его надо переотправить 
                // через обычный UDP сокет Ноды (node.send_udp_raw).
                // Но лучше просто настроить маршруты так, чтобы 4242 сюда не попадал.
                continue;
            }

            let dest_ip = format!("{}.{}.{}.{}", buf[16], buf[17], buf[18], buf[19]);
            let packet = buf[0..n].to_vec();
            self.clone().send_packet(dest_ip, packet);
        }
    }

    fn send_packet (self: Arc<Self>, dest_ip: String, packet: Vec<u8>) {
        let ip_to_peer_id = self.ip_to_peer_id.read().unwrap();
        let peer_id = ip_to_peer_id.get(&dest_ip);
        if let Some(peer_id) = peer_id {
            self.node.write().unwrap().send_packet(peer_id, packet);
        }
    }
}