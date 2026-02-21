use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use std::process::Command;
use dns_lookup::lookup_host;
use std::io::Read;

use crate::constructors::node::Node;

pub struct Sniffer {
    node: Node,
    target_hosts: Vec<String>,
    // Мапа обернута для безопасного доступа из разных потоков
    ip_to_peer_id: Arc<RwLock<HashMap<String, String>>>,
}

impl Sniffer {

    pub fn new (node: Node, target_hosts: Vec<String>) -> Sniffer {
        return Sniffer {
            ip_to_peer_id: Arc::new(RwLock::new(HashMap::new())),
            node,
            target_hosts
        }
    }

    pub fn start(self) {
        let ip_to_peer_id_clone = Arc::clone(&self.ip_to_peer_id);
        let node_link = self.node.clone();
        let targets = self.target_hosts.clone();

        // --- ПОТОК 1: РЕВИЗОР (раз в 10 секунд) ---
        thread::spawn(move || {
            loop {
                for host in &targets {
                    // Проверяем в Node: доступен ли пир для этого хоста?
                    if let Some(peer_id) = node_link.get_peer_id_by_host(host) {
                        if let Ok(ips) = lookup_host(host) {
                            for ip in ips {
                                let ip_str = ip.to_string();
                                
                                // Если этого IP еще нет в нашей мапе — добавляем и ставим маршрут
                                let mut map = ip_to_peer_id_clone.write().unwrap();
                                if !map.contains_key(&ip_str) {
                                    map.insert(ip_str.clone(), peer_id.clone());
                                    
                                    // Прописываем маршрут в ОС
                                    Command::new("ip")
                                        .args(&["route", "add", &ip_str, "dev", "tun0"])
                                        .status()
                                        .ok();
                                    
                                    println!("+ Маршрут: {} -> пир {}", ip_str, peer_id);
                                }
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_secs(10));
            }
        });

        // --- ПОТОК 2: ТРАФИК (основной) ---
        let mut config = tun::Configuration::default();
        config.up();
        let mut dev = tun::create(&config).expect("Failed to create TUN");

        let mut buf = [0; 4096];
        loop {
            let n = dev.read(&mut buf).unwrap();
            if n < 20 { continue; }

            // Вытаскиваем IP из пакета
            let dest_ip = format!("{}.{}.{}.{}", buf[16], buf[17], buf[18], buf[19]);

            // Проверяем по мапе (только на чтение - read() очень быстрый)
            let peer_id_opt = {
                let map = self.ip_to_peer_id.read().unwrap();
                map.get(&dest_ip).cloned()
            };

            if let Some(peer_id) = peer_id_opt {
                let data = buf[0..n].to_vec();
                // Отправляем через ноду (ей по-прежнему похуй, она просто транспорт)
                self.node.send_packet(&peer_id, data);
            }
        }
    }
}