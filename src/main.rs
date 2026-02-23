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
    collections::hash_map::DefaultHasher, error::Error, hash::{Hash, Hasher}, str::FromStr, time::{Duration}
};

use futures::stream::StreamExt;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol, autonat, dcutr, gossipsub, identify, kad::{self, RecordKey}, mdns, multiaddr::Protocol, noise, ping, relay, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux
};
use tokio::{io, io::AsyncBufReadExt, select};
use tracing_subscriber::{EnvFilter};
use kad::store::MemoryStore;

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    kademilia: kad::Behaviour<MemoryStore>,
    autonat: autonat::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour
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
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|key, relay| {
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

            let ping_config = ping::Config::new()
                .with_interval(std::time::Duration::from_secs(5)) // Пингуем каждые 15 сек
                .with_timeout(std::time::Duration::from_secs(1));
            let ping = ping::Behaviour::new(ping_config);
            
            let identify_config = identify::Config::new(
                "/ipfs/id/1.0.0".to_string(),
                key.public(),
            ).with_push_listen_addr_updates(true);
            let identify = identify::Behaviour::new(identify_config);

            let store = MemoryStore::new(key.public().to_peer_id());
            let mut cfg = kad::Config::default();
            let protos = vec![IPFS_PROTO_NAME];
            cfg.set_protocol_names(protos);
            let kademilia = kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg);

            let autonat_config = autonat::Config {
                retry_interval: Duration::from_secs(10),
                refresh_interval: Duration::from_secs(30),
                boot_delay: Duration::from_secs(5),
                ..Default::default()
            };

            let autonat = autonat::Behaviour::new(
                key.public().to_peer_id(),
                autonat_config,
            );

            // let relay_config = relay::Config::default();
            // let relay = relay::Behaviour::new(key.public().to_peer_id(), relay_config);

            let dcutr = dcutr::Behaviour::new(key.public().to_peer_id());
            
            Ok(MyBehaviour { 
                gossipsub, 
                mdns,
                ping,
                identify,
                kademilia,
                autonat,
                relay,
                dcutr
            })
        })?
        .build();
    
    let bootstraps = [
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
        "/dnsaddr/va1.bootstrap.libp2p.io/p2p/12D3KooWKnDdG3iXw9eTFijk3EWSunZcFi54Zka4wmtqtt6rPxc8",
        "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
        "/ip4/104.131.131.82/udp/4001/quic-v1/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ"
    ];

    for addr_str in bootstraps {
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

    swarm.behaviour_mut()
        .kademilia.set_mode(Some(libp2p::kad::Mode::Client));

    let topic_key = RecordKey::new(&topic.to_string());

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // === LISTENERS ===
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

    let relays = vec![
        "/ip4/107.174.64.174/udp/4001/quic-v1/p2p/12D3KooWK7tafwD96QWVGcEESBHx5nNVRTFDxidEZdTKQYjHpG9x"
    ];

    for relay_str in relays.clone() {
        let relay_arr: Vec<&str> = relay_str.split("/p2p/").collect();
        let relay_addr = Multiaddr::from_str(relay_arr[0]).unwrap();
        let relay_peer_id = PeerId::from_str(relay_arr[1]).unwrap();

        let target = relay_addr.clone()
            .with(Protocol::P2p(relay_peer_id))
            .with(Protocol::P2pCircuit);

        swarm.listen_on(target.clone())?;
        println!("📡 Subscribe to relays {}", relay_str);
    }

    let local_peer_id = *swarm.local_peer_id();
    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");
    println!("Swarm local peer id {}", local_peer_id.clone());
    // Kick it off
    let mut stats_timer = tokio::time::interval(std::time::Duration::from_secs(3));

    let mut last_provisioning = tokio::time::interval(std::time::Duration::from_mins(1));

    loop {
        select! {
            _ = last_provisioning.tick() => {
                swarm.behaviour_mut()
                    .kademilia.start_providing(topic_key.clone()).unwrap();
                swarm.behaviour_mut()
                    .kademilia.get_providers(topic_key.clone());
                println!("📡 Kademilia provisioning");
            }
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::ExternalAddrExpired { address } => {
                    println!("❌ Адрес протух: {address}");
                    // Тут НУЖНО заново вызвать listen_on на реле
                    // swarm.listen_on(target.clone()).unwrap();
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("📡 Слушаем на: {:?}", address);
                }
                SwarmEvent::IncomingConnection { .. } => {
                    println!("📥 Входящее соединение...");
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    for relay in relays.clone() {
                        if relay.contains(&peer_id.to_string()) {
                            println!("⚠️ Реле отключилось. Пробую переподключиться через 5 сек... {}", peer_id);
                        }
                    }
                }
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
                SwarmEvent::Behaviour(MyBehaviourEvent::Relay(relay::client::Event::ReservationReqAccepted {
                    .. 
                })) => {
                    println!("📡 Реле подключилось");
                    let ext_addresses: Vec<Multiaddr> =  swarm.external_addresses().cloned().collect();
                    let behaviour = swarm.behaviour_mut();
                    for ext in ext_addresses {
                        behaviour.kademilia.add_address(&local_peer_id, ext.clone());
                        println!("📡 Kademlia record {}:{}", &local_peer_id, &ext.clone().to_string());
                    }
                }
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => println!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::Behaviour(MyBehaviourEvent::Autonat(autonat::Event::StatusChanged { old, new })) => {
                    println!("🌐 Статус NAT изменился: {:?} -> {:?}", old, new);
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Dcutr(event)) => {
                    println!("🛠️ DCUtR Event: {:?}", event);
                }
                SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received { peer_id, info })) => {
                    for relay_addr in relays.clone() {
                        if relay_addr.contains(&peer_id.to_string()) {
                            println!("🛠 Протоколы реле: {:?}", peer_id);
                            for p in info.protocols.iter() {
                                println!("🚀 {}", p)
                            }
                        }
                    }
                }
                SwarmEvent::Behaviour(MyBehaviourEvent::Kademilia(kad::Event::OutboundQueryProgressed { result, .. })) => {
                    match result {
                        kad::QueryResult::GetProviders(Ok(ok)) => {
                            match ok {
                                kad::GetProvidersOk::FoundProviders { providers, .. } => {
                                    for peer_id in providers {
                                        if peer_id != local_peer_id {
                                            println!("📍 Нашел провайдера: {peer_id}. Пробую Dial...");
                                            let _ = swarm.dial(peer_id);
                                        }
                                    }
                                },
                                _ => {
                                    println!("🏁 Поиск завершен");
                                }
                            }
                        }
                        _ => {
                            println!("=== KAD PROGRESS: {:?} ===", result);
                        }
                    }
                }
                _ => {}
            },
            _ = stats_timer.tick() => {
                // clearscreen::clear().expect("failed to clear screen");

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

                // for (_, topics) in behaviour.gossipsub.all_peers() {
                //     for topic in topics {
                //         println!("Topic: {}", topic);
                //     }
                // }

                // 3. Общее кол-во активных соединений Swarm
                let active_connections = swarm.network_info().num_peers();

                println!("--- 📊 СТАТИСТИКА НОДЫ ---");
                println!("🌐 Соединений (Swarm)      : {}", active_connections);
                println!("📚 В таблице (Kademlia)    : {}", total_kad_peers);
                println!("💬 В сети (Gossipsub)      : {}", gossip_peers);
                println!("💬 В комнате (guardi-vpn)  : {}", room_peers);
                println!("--------------------------");
                println!("{:?}", swarm.external_addresses().collect::<Vec<_>>());
            }
        }
    }
}