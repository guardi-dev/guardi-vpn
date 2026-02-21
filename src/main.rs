mod constructors;

use constructors::node::Node;

fn main () {
    let mut target_hosts: Vec<String> = Vec::new();
    target_hosts.push("google.com".to_string());

    let node = Node::new(target_hosts);
    let mut node_close = node.clone();

    std::thread::spawn(move || {
        node_close.start(); // Твой бесконечный loop { recv_from... }
    });

}