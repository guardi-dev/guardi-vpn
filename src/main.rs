use std::thread;

use crate::constructors::{node::Node, proxy::Proxy};

mod constructors;

#[tokio::main]
async fn main () {
    let hosts: Vec<String> = vec!["google.com".to_string()];
    let node = Node::new(hosts.clone());
    let node_for_proxy = node.clone();
    let proxy = Proxy::new("0.0.0.0:8080".to_string(), hosts.clone(), node_for_proxy);

    thread::spawn(move || {
        Node::start(&node);
    });

    Proxy::start(proxy).await;
}