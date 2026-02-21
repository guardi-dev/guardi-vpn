mod constructors;

use constructors::node::Node;

fn main () {
    let mut target_hosts: Vec<String> = Vec::new();
    target_hosts.push("google.com".to_string());

    let mut node = Node::new(target_hosts);
    
    node.start();
}