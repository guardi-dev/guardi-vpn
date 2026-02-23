// Copyright 2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

#![doc = include_str!("../README.md")]

use std::{
    collections::hash_map::DefaultHasher, error::Error, hash::{Hash, Hasher}, str::FromStr, time::Duration
};

use futures::stream::StreamExt;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol, gossipsub, identify, kad, mdns, noise, ping, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux
};
use tokio::{io, io::AsyncBufReadExt, select};
use tracing_subscriber::EnvFilter;
use kad::store::MemoryStore;

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    kademilia: kad::Behaviour<MemoryStore>
}

const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_dns()?
        .with_behaviour(|key| {
            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub configuration
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message
                // signing)
                .mesh_n_low(2)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
                .build()
                .map_err(io::Error::other)?; // Temporary hack because `build` does not return a proper `std::error::Error`.

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

            let ping = ping::Behaviour::new(ping::Config::new());
            
            let identify = identify::Behaviour::new(identify::Config::new(
                "/ipfs/id/1.0.0".to_string(),
                key.public(),
            ));

            let store = MemoryStore::new(key.public().to_peer_id());
            let mut cfg = kad::Config::default();
            let protos = vec![IPFS_PROTO_NAME];
            cfg.set_protocol_names(protos);
            let kademilia = kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg);
            Ok(MyBehaviour { 
                gossipsub, 
                mdns,
                ping,
                identify,
                kademilia
            })
        })?
        .build();
    
    let bootstraps = [
        ("/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ", "Mars"),
        ("/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN", "Libp2p-org"),
        ("/ip4/147.75.83.83/tcp/4001/p2p/QmbLHAnMo9UFnmRR4zvKU912ZVeC8P7Bg3gixS6S8Xk8G9", "Protocol-Labs"),
    ];

    for (addr_str, _) in bootstraps {
        let addr_arr: Vec<&str> = addr_str.split("/").collect();
        let addr_short = addr_arr[..addr_arr.len().saturating_sub(2)].join("/");
        let addr = Multiaddr::from_str(&addr_short).unwrap();
        let peer_id_str = addr_arr.last().unwrap();
        let peer_id = PeerId::from_str(&peer_id_str).unwrap();

        swarm.behaviour_mut().kademilia.add_address(&peer_id, addr);
        swarm.dial(Multiaddr::from_str(&addr_str).unwrap()).ok(); // Сразу звоним им всем
    }


    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("guardi-vpn");
    // subscribes to our topic
    
    swarm.behaviour_mut()
        .gossipsub.subscribe(&topic)?;

    swarm.behaviour_mut()
        .kademilia.bootstrap().ok();

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    // let _ = swarm.dial("/ip4/104.131.131.82/udp/4001/quic-v1/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse::<Multiaddr>().unwrap());

    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");
    println!("Swarm local peer id {}", swarm.local_peer_id());

    // Kick it off
    let mut stats_timer = tokio::time::interval(std::time::Duration::from_secs(3));
    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discovered a new peer: {peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => println!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                },
                SwarmEvent::OutgoingConnectionError { .. } => {
                    // println!("Outgoing Connection Error {}", error);
                },
                _ => {}
            },
            _ = stats_timer.tick() => {
                clearscreen::clear().expect("failed to clear screen");

                let behaviour = swarm.behaviour_mut();
                let res = behaviour.gossipsub.publish(topic.clone(), format!("Hello world {}", rand::random::<u8>()));
                let _ = res.inspect_err(|e| eprintln!("Invalid publish: {e}"));

                // 1. Статистика Кадемлии (сколько пиров мы ЗНАЕМ)
                let mut total_kad_peers = 0;
                for bucket in behaviour.kademilia.kbuckets() {
                    total_kad_peers += bucket.num_entries();
                }

                // 2. Статистика Gossipsub (с кем мы реально БОЛТАЕМ)
                // Берем первый попавшийся топик для примера
                let gossip_peers = behaviour.gossipsub.all_peers().count();
                let room_peers = behaviour.gossipsub.all_peers().filter(|(_,t)| t.contains(&&topic.hash())).count();

                for (_, topics) in behaviour.gossipsub.all_peers() {
                    for topic in topics {
                        println!("Topic: {}", topic);
                    }
                }

                // 3. Общее кол-во активных соединений Swarm
                let active_connections = swarm.network_info().num_peers();

                println!("--- 📊 СТАТИСТИКА НОДЫ ---");
                println!("🌐 Соединений (Swarm)      : {}", active_connections);
                println!("📚 В таблице (Kademlia)    : {}", total_kad_peers);
                println!("💬 В сети (Gossipsub)      : {}", gossip_peers);
                println!("💬 В комнате (guardi-vpn)  : {}", room_peers);
                println!("--------------------------");
            }
        }
    }
}