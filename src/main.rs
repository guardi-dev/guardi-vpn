mod constructors;

use constructors::node::Node;
use constructors::sniffer::Sniffer;

fn main () {
    let mut target_hosts: Vec<String> = Vec::new();
    target_hosts.push("google.com".to_string());

    let node = Node::new(target_hosts.clone());
    let mut node_close = node.clone();

    std::thread::spawn(move || {
        node_close.start(); // Твой бесконечный loop { recv_from... }
    });

    let sniffer = Sniffer::new(node, target_hosts.clone());
    sniffer.start();
}