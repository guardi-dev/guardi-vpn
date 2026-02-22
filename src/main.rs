mod constructors;

use constructors::node::Node;
use constructors::sniffer::Sniffer;
use std::panic;
use std::process;
use std::sync::Arc;

fn main () {
    let mut target_hosts: Vec<String> = Vec::new();
    target_hosts.push("google.com".to_string());

    let node = Node::new(target_hosts.clone());
    let node_for_sniffer = Arc::clone(&node);

    panic::set_hook(Box::new(|panic_info| {
        eprintln!("Критическая ошибка в потоке: {:?}", panic_info);
        // Грохаем весь процесс с кодом ошибки
        process::exit(1);
    }));

    std::thread::spawn(move || {
        Node::start(&node);
    });

    let sniffer = Sniffer::new(node_for_sniffer, target_hosts.clone());
    Sniffer::start(sniffer);
}