//! Peer discovery for P2P network.
//!
//! Discovers peers through:
//! - Control plane API (primary method in Phase 1)
//! - mDNS/DNS-SD (Phase 2, for local network without control plane)
//!
//! Falls back to control-plane peer list if mDNS is unavailable.

use std::sync::Arc;

use dashmap::DashMap;
use tracing::{debug, info};

use super::peer::{PeerId, PeerInfo};
use super::piece::PieceTracker;

/// Discovers and manages peer connections.
///
/// In Phase 1, peers are discovered via the control plane's node list API.
/// Peers can also be added manually for testing and bootstrapping.
pub struct PeerDiscovery {
    /// Peer information table (shared with other P2P components)
    peers: Arc<DashMap<PeerId, PeerInfo>>,
    /// Piece availability tracker (shared with other P2P components)
    tracker: Arc<PieceTracker>,
    /// Control plane URL for peer discovery
    #[allow(dead_code)]
    control_plane_url: String,
}

impl PeerDiscovery {
    /// Create a new peer discovery instance.
    ///
    /// # Arguments
    /// * `peers` - Shared peer table
    /// * `tracker` - Shared piece tracker
    pub fn new(
        peers: Arc<DashMap<PeerId, PeerInfo>>,
        tracker: Arc<PieceTracker>,
    ) -> Result<Self, crate::RegistryError> {
        let control_plane_url =
            std::env::var("SHELLWEGO_CONTROL_PLANE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string());

        Ok(Self {
            peers,
            tracker,
            control_plane_url,
        })
    }

    /// Refresh the peer list from the control plane.
    ///
    /// In production, this fetches the list of registered nodes from the
    /// control plane API and updates the peer table. For now, this is a
    /// no-op (peers are added manually or via tests).
    pub async fn refresh(&self) -> Result<(), crate::RegistryError> {
        // In production: fetch peer list from control plane API
        // GET /v1/nodes -> list of {id, address, available_layers, ...}
        //
        // For each node:
        //   1. Update peer info in the table
        //   2. Register their available pieces in the tracker
        //
        debug!("Peer discovery: {} peers known", self.peers.len());
        Ok(())
    }

    /// Manually add a peer to the network.
    ///
    /// Useful for testing and initial bootstrapping when the control plane
    /// hasn't fully started yet.
    pub fn add_peer(&self, peer: PeerInfo) {
        debug!("Adding peer {} at {}", peer.id, peer.address);

        // Register their available pieces
        for layer_digest in &peer.available_layers {
            // We don't know the exact size, so we just register the peer's existence
            // The piece-level data will be populated when they announce pieces
            self.tracker.register_piece(layer_digest, 0, &peer.id);
        }

        self.peers.insert(peer.id.clone(), peer);
        info!(
            "Peer added: {} (total peers: {})",
            self.peers.len() - 1,
            self.peers.len()
        );
    }

    /// Remove a peer from the network.
    pub fn remove_peer(&self, peer_id: &PeerId) {
        if self.peers.remove(peer_id).is_some() {
            info!("Peer removed: {}", peer_id);
        }
    }

    /// Get the number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Check if a specific peer is known.
    pub fn has_peer(&self, peer_id: &PeerId) -> bool {
        self.peers.contains_key(peer_id)
    }

    /// Get a list of all known peer IDs.
    pub fn list_peers(&self) -> Vec<PeerId> {
        self.peers.iter().map(|r| r.key().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[tokio::test]
    async fn test_add_and_list_peers() {
        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let discovery = PeerDiscovery::new(peers.clone(), tracker.clone()).unwrap();

        assert_eq!(discovery.peer_count(), 0);

        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 31415));
        let peer = PeerInfo::new(PeerId("node-1".to_string()), addr);
        discovery.add_peer(peer);

        assert_eq!(discovery.peer_count(), 1);
        assert!(discovery.has_peer(&PeerId("node-1".to_string())));

        let list = discovery.list_peers();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_peer() {
        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let discovery = PeerDiscovery::new(peers.clone(), tracker.clone()).unwrap();

        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 31415));
        discovery.add_peer(PeerInfo::new(PeerId("node-1".to_string()), addr));
        assert_eq!(discovery.peer_count(), 1);

        discovery.remove_peer(&PeerId("node-1".to_string()));
        assert_eq!(discovery.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_refresh_noop() {
        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let discovery = PeerDiscovery::new(peers, tracker).unwrap();

        // Should not panic or error
        let result = discovery.refresh().await;
        assert!(result.is_ok());
    }
}
