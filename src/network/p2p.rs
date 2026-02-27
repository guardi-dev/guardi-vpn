use std::{
    collections::{hash_map::DefaultHasher}, error::Error, hash::{Hash, Hasher}, num::NonZero, str::FromStr, time::Duration
};

use futures::stream::StreamExt;
use libp2p::{
    Multiaddr, StreamProtocol, autonat, dcutr, gossipsub, identify, kad::{self, RecordKey}, multiaddr::Protocol, noise, ping, relay, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux
};
use tokio::{io, select};
use tracing_subscriber::{EnvFilter};
use kad::store::MemoryStore;
use uuid::Uuid;

use crate::network::{broadcast::{ChatMessage, EngineEvent, P2PBroadcast, StatsMessage}, xml::{MessageType, UserMessage, XML}};
use crate::logln;

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    kademilia: kad::Behaviour<MemoryStore>,
    autonat: autonat::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
}

const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const ID_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");
const GUARDI_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/guardi-vpn/1.0.0");
const TOPIC: &str = "guardi-vpn-v4";

pub struct P2PEngine {
    pub broadcast: P2PBroadcast
}

impl  P2PEngine {
    
    pub fn new () -> Self {
        return P2PEngine {
            broadcast: P2PBroadcast::new()
        }
    }

    pub async fn listen(&self) -> Result<(), Box<dyn Error>> {
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
                    .allow_self_origin(true)
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

                let ping_config = ping::Config::new()
                    .with_interval(std::time::Duration::from_secs(5)) // Пингуем каждые 15 сек
                    .with_timeout(std::time::Duration::from_secs(1));
                let ping = ping::Behaviour::new(ping_config);
                
                let identify_config = identify::Config::new(
                    ID_PROTO_NAME.to_string(),
                    key.public(),
                ).with_push_listen_addr_updates(true);
                let identify = identify::Behaviour::new(identify_config);

                let kad_store = MemoryStore::new(key.public().to_peer_id());
                let mut kad_cfg = kad::Config::default();
                kad_cfg.set_parallelism(NonZero::new(3 as usize).expect("Zero"));
                let protos = vec![
                    IPFS_PROTO_NAME,
                    ID_PROTO_NAME,
                    GUARDI_PROTO_NAME
                ];
                kad_cfg.set_protocol_names(protos);
                kad_cfg.set_replication_interval(Some(Duration::from_mins(5)));
                kad_cfg.set_provider_record_ttl(Some(Duration::from_mins(2)));
                kad_cfg.set_provider_publication_interval(Some(Duration::from_secs(10)));
                let kad = kad::Behaviour::with_config(key.public().to_peer_id(), kad_store, kad_cfg);

                let autonat_config = autonat::Config {
                    retry_interval: Duration::from_secs(10),
                    refresh_interval: Duration::from_secs(30),
                    boot_delay: Duration::from_secs(5),
                    use_connected: true,
                    ..Default::default()
                };

                let autonat = autonat::Behaviour::new(
                    key.public().to_peer_id(),
                    autonat_config,
                );

                let dcutr = dcutr::Behaviour::new(key.public().to_peer_id());

                Ok(MyBehaviour { 
                    gossipsub, 
                    ping,
                    identify,
                    kademilia: kad,
                    autonat,
                    relay,
                    dcutr
                })
            })?
            .build();

        // Create a Gossipsub topic
        let topic = gossipsub::IdentTopic::new(TOPIC);
        let topic_key = RecordKey::new(&TOPIC);

        // subscribes to our topic
        
        swarm.behaviour_mut()
            .gossipsub.subscribe(&topic)?;

        swarm.behaviour_mut()
            .kademilia.bootstrap().ok();

        swarm.behaviour_mut()
            .kademilia.set_mode(Some(libp2p::kad::Mode::Client));

        swarm.behaviour_mut()
            .kademilia.start_providing(topic_key.clone())?;

        // === LISTENERS ===
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

        let bootstraps: Vec<Multiaddr> = vec![
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
            "/dnsaddr/va1.bootstrap.libp2p.io/p2p/12D3KooWKnDdG3iXw9eTFijk3EWSunZcFi54Zka4wmtqtt6rPxc8",
            "/ip4/104.131.131.82/udp/4001/quic-v1/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ"
        ].iter().map(|i| {
            let addr = Multiaddr::from_str(i).unwrap();
            addr.clone().with(Protocol::P2pCircuit)
        }).collect();

        let local_peer_id = *swarm.local_peer_id();
        logln!(self, "Swarm local peer id {}", local_peer_id.clone());

        let mut stats_timer = tokio::time::interval(Duration::from_secs(10));

        let mut last_provisioning = tokio::time::interval(Duration::from_secs(30));

        let mut reconnect = tokio::time::interval(Duration::from_mins(2));

        let mut tx = self.broadcast.subscribe();

        loop {
            select! {   
                _ = reconnect.tick() => {
                    let listen_addresses = swarm.external_addresses().cloned().collect::<Vec<Multiaddr>>();
                    let listen_one_line = listen_addresses.iter().map(|i| i.to_string()).collect::<Vec<String>>().join("|");
                    for boot in bootstraps.clone() {
                        if listen_one_line.contains(&boot.to_string()) {
                            continue;
                        }
                        logln!(self, "📡 Reconnecting to relay {}", &boot);
                        let _ = swarm.listen_on(boot.clone());
                    }
                }
                _ = last_provisioning.tick() => {
                    let ext_count = swarm.external_addresses().count();
                    if ext_count > 0 {
                        logln!(self, "📡 Kademilia get providers");
                        swarm.behaviour_mut().kademilia.get_providers(topic_key.clone());
                    }
                }
                _ = stats_timer.tick() => {
                    let behaviour = swarm.behaviour_mut();
                    let mut total_kad_peers = 0;
                    for bucket in behaviour.kademilia.kbuckets() {
                        total_kad_peers += bucket.num_entries();
                    }

                    let gossip_peers = behaviour.gossipsub.all_peers().count();
                    let room_peers = behaviour.gossipsub.all_peers().filter(|(_,t)| t.contains(&&topic.hash())).count();
                    let ext_addresses: Vec<&Multiaddr> = swarm.external_addresses().collect();
                    let ext_addresses_count = ext_addresses.len();

                    logln!(self, "Guardi VPN 📚:{} 🌐:{} 📡:{} 💬:{}", 
                        total_kad_peers,
                        gossip_peers,
                        ext_addresses_count,
                        room_peers
                    );
                    
                    self.broadcast.on_stats(StatsMessage {
                        room_count: room_peers.try_into().unwrap(),
                        active_ralays_count: ext_addresses_count.try_into().unwrap(),
                        gosibsub_count: gossip_peers.try_into().unwrap(),
                        kademlia_count: total_kad_peers.try_into().unwrap()
                    });
                }
                tx_event = tx.recv() => {
                    match tx_event {
                        Ok(EngineEvent::User(msg)) => {
                            let xml = XML::write(&UserMessage {
                                id: &Uuid::new_v4().to_string(),
                                m: &msg.content,
                                t: MessageType::ChatPlain
                            });
                            if xml.is_err() {
                                let _ = xml.inspect_err(|e| logln!(self, "Invalid xml writing: {e}"));
                                continue;
                            }
                            let behaviour = swarm.behaviour_mut();
                            let res = behaviour.gossipsub.publish(topic.clone(), xml.unwrap());
                            let _ = res.inspect_err(|e| logln!(self, "Invalid publish: {e}"));
                        }
                        _ => {}
                    }
                }
                event = swarm.select_next_some() => match event {
                    SwarmEvent::IncomingConnection { .. } => {
                        logln!(self, "📥 Входящее соединение...");
                    }
                    SwarmEvent::IncomingConnectionError { error, local_addr, send_back_addr, .. } => {
                        logln!(self, "❌ Incoming connection error {error}");
                        logln!(self, "❌ Local {}", local_addr);
                        logln!(self, "❌ Send back {}", send_back_addr);
                    }
                    SwarmEvent::ExternalAddrConfirmed { .. } => {
                        logln!(self, "📡 External Addr Confirmed");
                    }
                    SwarmEvent::ExternalAddrExpired { address } => {
                        logln!(self, "❌ Remote address expired: {address}");
                    }
                    SwarmEvent::Behaviour(my_behaviour) => {
                        match my_behaviour {
                            MyBehaviourEvent::Gossipsub(gossipsub) => {
                                match gossipsub {
                                    gossipsub::Event::Message {
                                        propagation_source: peer_id,
                                        message,
                                        ..
                                    } => {
                                        let message_data = String::from_utf8_lossy(&message.data);
                                        logln!(self, "Got message: '{message_data}' from peer: {peer_id}");

                                        let json = XML::read::<UserMessage>(&message_data);
                                        if json.is_err() {
                                            let _ = json.inspect_err(|e| println!("Invalid parsing xml {}", e));
                                            continue;
                                        }

                                        let json = json.unwrap();
                                        self.broadcast.on_network_room_message(ChatMessage {
                                            id: json.id.to_string(),
                                            content: json.m.to_string(),
                                            sender: peer_id.to_string()
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            MyBehaviourEvent::Autonat(autonat) => {
                                match autonat {
                                    autonat::Event::StatusChanged { old, new } => {
                                        logln!(self, "🌐 Статус NAT изменился: {:?} -> {:?}", old, new);
                                    }
                                    _ => {}
                                }
                            }
                            MyBehaviourEvent::Dcutr(event) => {
                                logln!(self, "🛠️ DCUTR {:?}", event.result);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}