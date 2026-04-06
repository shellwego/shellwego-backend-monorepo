//! Dragonfly-inspired P2P image distribution client.
//!
//! High-level client that orchestrates P2P layer downloads using the
//! piece scheduler, transport, and peer discovery components. Falls back
//! gracefully to HTTP mirror chain or upstream registry if P2P is not available.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use sha2::{Digest as Sha256Digest, Sha256};
use tracing::{info, warn};

use super::discovery::PeerDiscovery;
use super::peer::PeerId;
use super::piece::PieceTracker;
use super::scheduler::PieceScheduler;
use super::transport::PeerTransport;
use crate::RegistryError;

/// Dragonfly-inspired P2P image distribution client.
///
/// Coordinates P2P layer downloads across multiple peers using
/// rarest-first piece scheduling. If not enough peers are available,
/// returns `None` to allow the caller to fall back to HTTP.
pub struct DragonflyClient {
    /// Peer information table (shared with other P2P components)
    peers: Arc<DashMap<PeerId, super::peer::PeerInfo>>,
    /// Piece availability tracker
    tracker: Arc<PieceTracker>,
    /// Piece scheduler
    scheduler: PieceScheduler,
    /// Transport layer
    transport: PeerTransport,
    /// Peer discovery
    discovery: PeerDiscovery,
    /// Local piece cache (serves pieces to other peers)
    local_pieces: Arc<DashMap<String, DashMap<u64, Bytes>>>,
    /// Minimum peers required before attempting P2P pull
    min_peers: usize,
}

impl DragonflyClient {
    /// Create a new Dragonfly P2P client.
    ///
    /// # Arguments
    /// * `min_peers` - Minimum number of peers with pieces before attempting P2P pull
    pub async fn new(min_peers: usize) -> Result<Self, RegistryError> {
        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let discovery = PeerDiscovery::new(peers.clone(), tracker.clone())?;

        Ok(Self {
            peers,
            tracker,
            scheduler,
            transport: PeerTransport::new(),
            discovery,
            local_pieces: Arc::new(DashMap::new()),
            min_peers,
        })
    }

    /// Try to pull a layer via P2P.
    ///
    /// Returns `Ok(Some(data))` if the layer was successfully downloaded
    /// from peers. Returns `Ok(None)` if P2P is not available (not enough
    /// peers, incomplete pieces, etc.) — the caller should fall back to HTTP.
    pub async fn pull_layer(
        &self,
        digest: &str,
        expected_size: u64,
    ) -> Result<Option<Bytes>, RegistryError> {
        // Refresh peer list
        self.discovery.refresh().await?;

        // Check if we have enough peers with pieces
        let available_peers = self.count_available_peers(digest);
        if available_peers < self.min_peers {
            info!(
                "Not enough P2P peers for layer {} (have {}, need {})",
                digest, available_peers, self.min_peers
            );
            return Ok(None);
        }

        // Schedule pieces
        let assignments = self.scheduler.schedule(digest, expected_size);
        if assignments.is_empty() {
            info!("No pieces scheduled for layer {}", digest);
            return Ok(None);
        }

        let peer_set: HashSet<_> = assignments.iter().map(|a| a.peer_id.0.clone()).collect();
        info!(
            "P2P pull: {} pieces scheduled for layer {} (from {} peers)",
            assignments.len(),
            digest,
            peer_set.len()
        );

        // Download pieces concurrently
        let mut piece_results: Vec<(u64, u64, Bytes)> = Vec::new();
        let mut handles = Vec::new();

        for assignment in assignments {
            let transport = PeerTransport::new();
            let peer = self
                .peers
                .get(&assignment.peer_id)
                .map(|p| p.clone())
                .ok_or_else(|| {
                    RegistryError::PullFailed(format!("Peer {} not found", assignment.peer_id.0))
                })?;

            let digest_clone = assignment.digest.clone();
            let offset = assignment.offset;
            let length = assignment.length;

            handles.push(tokio::spawn(async move {
                (
                    offset,
                    length,
                    transport.fetch_piece(peer.address, &digest_clone, offset, length).await,
                )
            }));
        }

        // Collect results
        for handle in handles {
            match handle.await {
                Ok((offset, length, Ok(data))) => {
                    piece_results.push((offset, length, data));
                }
                Ok((offset, _, Err(e))) => {
                    warn!("P2P piece download failed at offset {}: {}", offset, e);
                }
                Err(e) => {
                    warn!("P2P task panicked: {}", e);
                }
            }
        }

        // Check completion ratio
        let total_pieces = (expected_size + 1_048_576 - 1) / 1_048_576;
        let completion_ratio = piece_results.len() as f64 / total_pieces as f64;

        if completion_ratio < 0.8 {
            info!(
                "P2P completion only {:.0}% for {}, falling back to HTTP",
                completion_ratio * 100.0,
                digest
            );
            return Ok(None);
        }

        // Require 100% piece completion (conservative for Phase 1)
        if piece_results.len() != total_pieces as usize {
            info!(
                "P2P incomplete: {}/{} pieces for {}, falling back to HTTP",
                piece_results.len(),
                total_pieces,
                digest
            );
            return Ok(None);
        }

        // Reassemble pieces into full layer
        piece_results.sort_by_key(|(offset, _, _)| *offset);
        let layer_bytes: Bytes = piece_results
            .into_iter()
            .flat_map(|(_, _, data)| data.to_vec())
            .collect::<Vec<u8>>()
            .into();

        // Verify digest
        let computed = format!("sha256:{:x}", Sha256::digest(&layer_bytes));
        if computed != digest {
            warn!(
                "P2P digest mismatch: expected {}, got {}",
                digest, computed
            );
            return Ok(None);
        }

        // Cache local pieces for serving to other peers
        self.cache_local_pieces(digest, &layer_bytes);

        info!(
            "P2P pull complete for layer {} ({} bytes)",
            digest,
            layer_bytes.len()
        );
        Ok(Some(layer_bytes))
    }

    /// Count unique peers that have at least one piece of the given layer.
    fn count_available_peers(&self, digest: &str) -> usize {
        self.tracker
            .peers_for_layer(digest)
            .len()
    }

    /// Cache pieces locally for serving to other peers.
    fn cache_local_pieces(&self, digest: &str, data: &[u8]) {
        let layer_pieces = self
            .local_pieces
            .entry(digest.to_string())
            .or_default();
        let mut offset = 0u64;
        while offset < data.len() as u64 {
            let end = std::cmp::min(offset + 1_048_576, data.len() as u64);
            layer_pieces.insert(
                offset,
                Bytes::copy_from_slice(&data[offset as usize..end as usize]),
            );
            offset = end;
        }
    }

    /// Announce to the network that we have a layer available.
    ///
    /// Call this after successfully pulling a layer (via P2P or HTTP)
    /// so other peers can download pieces from us.
    pub async fn announce_layer(&self, digest: &str, total_size: u64) {
        let total_pieces = (total_size + 1_048_576 - 1) / 1_048_576;

        // Register all pieces as available from this peer
        // (This node acts as a seed for other peers)
        debug!(
            "Announcing layer {} with {} pieces to P2P network",
            digest, total_pieces
        );

        // Update the tracker with our local pieces
        for i in 0..total_pieces {
            self.tracker
                .register_piece(digest, i * 1_048_576, &self.local_peer_id());
        }
    }

    /// Get the local peer ID.
    fn local_peer_id(&self) -> PeerId {
        // In production, this would be derived from a persistent keypair
        PeerId(
            std::env::var("SHELLWEGO_NODE_ID")
                .unwrap_or_else(|_| "local-node".to_string()),
        )
    }

    /// Add a peer manually (for testing or bootstrapping).
    pub fn add_peer(&self, peer: super::peer::PeerInfo) {
        self.discovery.add_peer(peer);
    }

    /// Get the number of known peers.
    pub fn peer_count(&self) -> usize {
        self.discovery.peer_count()
    }

    /// Check if local piece cache has data for a specific piece.
    pub fn has_local_piece(&self, digest: &str, offset: u64) -> bool {
        self.local_pieces
            .get(digest)
            .map(|layer| layer.contains_key(&offset))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = DragonflyClient::new(2).await;
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_pull_layer_no_peers() {
        let client = DragonflyClient::new(1).await.unwrap();
        let result = client.pull_layer("sha256:abc", 1_000_000).await;
        // Should return None since no peers are available
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
