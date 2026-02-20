use crate::{PeerId, RouteScore};

/// L: pick the peer with the lowest weight
pub fn pick_best(candidates: Vec<(PeerId, RouteScore)>) -> Option<PeerId> {
    // We look for the minimum score in the list
    candidates
        .into_iter()
        .min_by_key(|&(_, score)| score)
        .map(|(id, _)| id)
}