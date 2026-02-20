TYPES
|__ PeerId        | D: String
|__ Latency       | D: u32
|__ Hostname      | D: String
|__ RouteScore    | D: u32
|__ Packet        | D: []u8
|__ PeerList      | D: []PeerId

root
|__ main.rs | L: run p2p node and intercept system traffic simultaneously
     |__ network/*
          |__ ping.rs    | I: PeerId  | O: Latency | L: measure response time to a peer
          |__ score.rs   | I: Latency, Latency | O: RouteScore | L: calculate path weight based on peer and target delays
          |__ select.rs  | I: []RouteScore | O: PeerId | L: pick the peer with the lowest weight
     |__ interceptor/*
          |__ sniffer.rs | I: u16 (port) | O: Packet | L: capture inbound packet from port via driver
          |__ parser.rs  | I: Packet | O: Hostname | L: extract domain name from packet header
     |__ tunnel/*
          |__ proxy.rs   | I: TcpStream, PeerId | L: open p2p stream to peer and bridge it with local connection
