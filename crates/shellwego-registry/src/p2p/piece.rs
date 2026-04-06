//! Piece-level availability tracking.
//!
//! Tracks which 1MB pieces of each layer are available from which peers.
//! Uses a bitfield-like structure with `DashMap` for lock-free concurrent access.

use std::collections::{HashMap, HashSet};

use dashmap::DashMap;

use super::peer::PeerId;

/// Default piece size: 1MB.
pub const PIECE_SIZE: usize = 1024 * 1024;

/// Tracks piece availability across peers for a layer.
///
/// Structure: `layer_digest -> piece_offset -> set of peer IDs`
pub struct PieceTracker {
    /// Layer digest → piece offset → set of peer IDs that have this piece
    pieces: DashMap<String, DashMap<u64, HashSet<PeerId>>>,
    /// Total pieces per layer (digest → count)
    piece_counts: DashMap<String, u64>,
}

impl PieceTracker {
    /// Create a new empty piece tracker.
    pub fn new() -> Self {
        Self {
            pieces: DashMap::new(),
            piece_counts: DashMap::new(),
        }
    }

    /// Register that a peer has a specific piece of a layer.
    pub fn register_piece(&self, digest: &str, offset: u64, peer_id: &PeerId) {
        let layer_pieces = self.pieces.entry(digest.to_string()).or_default();
        let peers = layer_pieces.entry(offset).or_default();
        peers.insert(peer_id.clone());

        // Track piece count
        let piece_index = offset / PIECE_SIZE as u64;
        if let Some(mut count) = self.piece_counts.get_mut(digest) {
            if piece_index + 1 > *count {
                *count = piece_index + 1;
            }
        } else {
            self.piece_counts
                .insert(digest.to_string(), piece_index + 1);
        }
    }

    /// Register all pieces for a layer that a peer has.
    pub fn register_layer(&self, digest: &str, total_size: u64, peer_id: &PeerId) {
        let total_pieces = (total_size + PIECE_SIZE as u64 - 1) / PIECE_SIZE as u64;
        for i in 0..total_pieces {
            self.register_piece(digest, i * PIECE_SIZE as u64, peer_id);
        }
    }

    /// Get peers that have a specific piece, sorted by rarity (rarest first).
    pub fn get_peers_for_piece(&self, digest: &str, offset: u64) -> Vec<PeerId> {
        self.pieces
            .get(digest)
            .and_then(|layer| layer.get(&offset))
            .map(|peers| {
                let mut v: Vec<_> = peers.iter().cloned().collect();
                // Sort by rarity (peers with fewer total pieces = rarer)
                v.sort_by_key(|peer| self.peer_piece_count(peer));
                v
            })
            .unwrap_or_default()
    }

    /// Check if a specific piece exists at all in the network.
    pub fn piece_available(&self, digest: &str, offset: u64) -> bool {
        self.pieces
            .get(digest)
            .map(|layer| layer.contains_key(&offset))
            .unwrap_or(false)
    }

    /// Get the total number of pieces for a layer.
    pub fn piece_count(&self, digest: &str) -> u64 {
        self.piece_counts
            .get(digest)
            .map(|c| *c)
            .unwrap_or(0)
    }

    /// Count how many unique pieces a peer has across all layers.
    fn peer_piece_count(&self, peer_id: &PeerId) -> usize {
        let mut count = 0;
        for layer_entry in self.pieces.iter() {
            for piece_entry in layer_entry.value().iter() {
                if piece_entry.value().contains(peer_id) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Get all unique peer IDs that have pieces for a given layer.
    pub fn peers_for_layer(&self, digest: &str) -> Vec<PeerId> {
        let mut peer_ids = HashSet::new();
        if let Some(layer) = self.pieces.get(digest) {
            for entry in layer.iter() {
                for peer in entry.value().iter() {
                    peer_ids.insert(peer.clone());
                }
            }
        }
        peer_ids.into_iter().collect()
    }

    /// Remove all piece tracking for a layer.
    pub fn remove_layer(&self, digest: &str) {
        self.pieces.remove(digest);
        self.piece_counts.remove(digest);
    }

    /// Get the number of layers being tracked.
    pub fn layer_count(&self) -> usize {
        self.pieces.len()
    }
}

impl Default for PieceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer_id(s: &str) -> PeerId {
        PeerId(s.to_string())
    }

    #[test]
    fn test_register_and_get_piece() {
        let tracker = PieceTracker::new();
        let peer1 = make_peer_id("node-1");

        tracker.register_piece("sha256:abc", 0, &peer1);

        let peers = tracker.get_peers_for_piece("sha256:abc", 0);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0], peer1);
    }

    #[test]
    fn test_piece_availability() {
        let tracker = PieceTracker::new();

        assert!(!tracker.piece_available("sha256:abc", 0));

        tracker.register_piece("sha256:abc", 0, &make_peer_id("node-1"));
        assert!(tracker.piece_available("sha256:abc", 0));
    }

    #[test]
    fn test_register_layer() {
        let tracker = PieceTracker::new();
        let peer1 = make_peer_id("node-1");

        // 2.5 MB layer = 3 pieces
        tracker.register_layer("sha256:abc", 2_500_000, &peer1);

        assert_eq!(tracker.piece_count("sha256:abc"), 3);
        assert!(tracker.piece_available("sha256:abc", 0));
        assert!(tracker.piece_available("sha256:abc", 1_048_576));
        assert!(tracker.piece_available("sha256:abc", 2_097_152));
    }

    #[test]
    fn test_rarest_first_ordering() {
        let tracker = PieceTracker::new();
        let peer1 = make_peer_id("node-1");
        let peer2 = make_peer_id("node-2");

        // Peer1 has layer A only (rare for piece of layer B)
        tracker.register_piece("sha256:layerA", 0, &peer1);
        // Peer2 has both layers
        tracker.register_piece("sha256:layerA", 0, &peer2);
        tracker.register_piece("sha256:layerB", 0, &peer2);

        // For layerA piece 0, peer1 should come first (fewer total pieces)
        let peers = tracker.get_peers_for_piece("sha256:layerA", 0);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0], peer1); // Rarer peer first
    }

    #[test]
    fn test_peers_for_layer() {
        let tracker = PieceTracker::new();

        tracker.register_piece("sha256:abc", 0, &make_peer_id("node-1"));
        tracker.register_piece("sha256:abc", 1_048_576, &make_peer_id("node-2"));

        let peers = tracker.peers_for_layer("sha256:abc");
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_remove_layer() {
        let tracker = PieceTracker::new();
        tracker.register_piece("sha256:abc", 0, &make_peer_id("node-1"));
        assert_eq!(tracker.layer_count(), 1);

        tracker.remove_layer("sha256:abc");
        assert_eq!(tracker.layer_count(), 0);
        assert!(!tracker.piece_available("sha256:abc", 0));
    }
}
