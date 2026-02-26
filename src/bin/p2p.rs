use guardi_vpn::network::p2p::P2PEngine;

#[tokio::main]
async fn main() {
  println!("Start p2p engine");
  let p2p = P2PEngine::new();
  p2p.listen().await.unwrap();
}