use crate::{Latency, RouteScore};

/// L: calculate path weight based on peer and target delays
/// Formula: (PeerLatency * 0.6) + (TargetLatency * 0.4)
pub fn calculate(peer_lat: Latency, target_lat: Latency) -> RouteScore {
    let p = peer_lat as f64;
    let t = target_lat as f64;
    
    // Applying the weights to determine the best routing candidate
    let score = (p * 0.6) + (t * 0.4);
    
    score as RouteScore
}