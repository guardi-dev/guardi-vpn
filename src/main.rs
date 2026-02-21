mod constructors;

use constructors::node::Node;

fn main () {
    let mut node = Node::new();
    node.start();
}