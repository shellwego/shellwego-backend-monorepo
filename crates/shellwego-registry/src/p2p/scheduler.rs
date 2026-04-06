//! Piece scheduler using Dragonfly's rarest-first strategy.
//!
//! Selects the optimal peer for each piece based on:
//! 1. Piece rarity (rarest pieces are scheduled first to preserve distribution)
//! 2. Peer bandwidth (higher bandwidth peers are preferred)
//! 3. Per-peer concurrency limits (avoid overwhelming any single peer)

use std::collections::HashSet;
use std::sync::Arc;

use dashmap::DashMap;
use tracing::debug;

use super::peer::{PeerId, PeerInfo};
use super::piece::PieceTracker;

/// Default piece size for scheduling: 1MB.
const PIECE_SIZE: u64 = 1024 * 1024;
/// Default maximum concurrent downloads per peer.
const DEFAULT_CONCURRENT_PER_PEER: u32 = 4;

/// Schedules piece downloads using Dragonfly's rarest-first strategy.
pub struct PieceScheduler {
    /// Piece availability tracker
    tracker: Arc<PieceTracker>,
    /// Peer information table
    peers: Arc<DashMap<PeerId, PeerInfo>>,
    /// Concurrency limit per peer
    max_concurrent_per_peer: u32,
}

impl PieceScheduler {
    /// Create a new piece scheduler.
    ///
    /// # Arguments
    /// * `tracker` - Piece availability tracker
    /// * `peers` - Peer information table
    /// * `max_global_concurrent` - Maximum concurrent downloads overall (unused in Phase 1)
    pub fn new(
        tracker: Arc<PieceTracker>,
        peers: Arc<DashMap<PeerId, PeerInfo>>,
        _max_global_concurrent: usize,
    ) -> Self {
        Self {
            tracker,
            peers,
            max_concurrent_per_peer: DEFAULT_CONCURRENT_PER_PEER,
        }
    }

    /// Schedule pieces for a layer download.
    ///
    /// Returns a list of `PieceAssignment` structs, each mapping a piece
    /// to a specific peer. Pieces are scheduled in rarest-first order,
    /// with the highest-bandwidth peer selected for each piece.
    pub fn schedule(&self, digest: &str, total_size: u64) -> Vec<PieceAssignment> {
        let total_pieces = (total_size + PIECE_SIZE - 1) / PIECE_SIZE;
        let mut assignments = Vec::new();
        let mut assigned_offsets: HashSet<u64> = HashSet::new();

        // Collect all pieces with their peer options
        let mut pieces: Vec<(u64, Vec<PeerId>)> = Vec::new();
        for offset in 0..total_pieces {
            let peers = self.tracker.get_peers_for_piece(digest, offset * PIECE_SIZE);
            if !peers.is_empty() {
                pieces.push((offset, peers));
            }
        }

        // Sort by rarity (fewer available peers = higher priority)
        pieces.sort_by_key(|(_, peers)| peers.len());

        for (offset, mut peer_options) in pieces {
            if assigned_offsets.contains(&offset) {
                continue;
            }

            // Pick the best peer: highest bandwidth, under concurrency limit
            peer_options.sort_by(|a, b| {
                let bw_a = self
                    .peers
                    .get(a)
                    .map(|p| p.bandwidth_bps)
                    .unwrap_or(0);
                let bw_b = self
                    .peers
                    .get(b)
                    .map(|p| p.bandwidth_bps)
                    .unwrap_or(0);
                bw_b.cmp(&bw_a) // highest bandwidth first
            });

            if let Some(peer_id) = peer_options.into_iter().find(|p| {
                self.peers
                    .get(p)
                    .map(|info| info.current_downloads < self.max_concurrent_per_peer)
                    .unwrap_or(false)
            }) {
                let piece_length = std::cmp::min(PIECE_SIZE, total_size - offset * PIECE_SIZE);
                assignments.push(PieceAssignment {
                    peer_id,
                    digest: digest.to_string(),
                    offset: offset * PIECE_SIZE,
                    length: piece_length,
                });
                assigned_offsets.insert(offset);
            }
        }

        debug!(
            "Scheduled {}/{} pieces from P2P for layer {}",
            assignments.len(),
            total_pieces,
            digest
        );
        assignments
    }
}

/// A piece download assignment: which peer should provide which piece.
#[derive(Debug, Clone)]
pub struct PieceAssignment {
    /// The peer to download from
    pub peer_id: PeerId,
    /// Layer digest
    pub digest: String,
    /// Byte offset within the layer
    pub offset: u64,
    /// Length of this piece in bytes
    pub length: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn make_peer(id: &str, bps: u64) -> PeerInfo {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 31415));
        PeerInfo {
            id: PeerId(id.to_string()),
            address: addr,
            available_layers: Vec::new(),
            bandwidth_bps: bps,
            rtt_ms: 10,
            max_concurrent: 4,
            current_downloads: 0,
            last_seen: std::time::Instant::now(),
        }
    }

    #[test]
    fn test_schedule_basic() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let peer1 = make_peer("node-1", 100_000_000);
        peers.insert(peer1.id.clone(), peer1);

        // Register pieces for a 2MB layer (2 pieces)
        tracker.register_layer("sha256:abc", 2_000_000, &PeerId("node-1".to_string()));

        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let assignments = scheduler.schedule("sha256:abc", 2_000_000);

        assert_eq!(assignments.len(), 2);
    }

    #[test]
    fn test_schedule_rarest_first() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let peer1 = make_peer("node-1", 100_000_000);
        let peer2 = make_peer("node-2", 200_000_000);
        peers.insert(peer1.id.clone(), peer1);
        peers.insert(peer2.id.clone(), peer2);

        // Piece 0: only node-1 has it (rare)
        tracker.register_piece("sha256:abc", 0, &PeerId("node-1".to_string()));
        // Piece 1: both have it (common)
        tracker.register_piece("sha256:abc", PIECE_SIZE, &PeerId("node-1".to_string()));
        tracker.register_piece("sha256:abc", PIECE_SIZE, &PeerId("node-2".to_string()));

        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let assignments = scheduler.schedule("sha256:abc", 2 * PIECE_SIZE);

        // Both pieces should be assigned
        assert_eq!(assignments.len(), 2);

        // First assignment should be for the rare piece (offset 0)
        assert_eq!(assignments[0].offset, 0);
    }

    #[test]
    fn test_schedule_empty() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let scheduler = PieceScheduler::new(tracker, peers, 16);
        let assignments = scheduler.schedule("sha256:abc", 1_000_000);

        assert!(assignments.is_empty());
    }
}
