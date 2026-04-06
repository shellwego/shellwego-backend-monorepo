//! Integration tests for P2P distribution.
//!
//! Tests simulated peer networks, piece tracking, scheduling, and fallback behavior.

#[cfg(test)]
mod tests {
    use shellwego_registry::p2p::peer::{PeerId, PeerInfo};
    use shellwego_registry::p2p::piece::PieceTracker;
    use shellwego_registry::p2p::scheduler::PieceScheduler;
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::sync::Arc;
    use dashmap::DashMap;

    fn make_peer_addr(port: u16) -> std::net::SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), port))
    }

    fn make_peer(id: &str, port: u16, bps: u64) -> PeerInfo {
        PeerInfo::new(PeerId(id.to_string()), make_peer_addr(port))
    }

    #[test]
    fn test_peer_id_equality() {
        let a = PeerId("node-1".to_string());
        let b = PeerId("node-1".to_string());
        let c = PeerId("node-2".to_string());
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(format!("{}", a), "node-1");
    }

    #[tokio::test]
    async fn test_piece_tracker_register_and_query() {
        let tracker = PieceTracker::new();
        let peer = PeerId("node-1".to_string());

        // Register pieces for a 3MB layer (3 pieces of 1MB)
        tracker.register_layer("sha256:abc", 3_000_000, &peer);

        assert!(tracker.piece_available("sha256:abc", 0));
        assert!(tracker.piece_available("sha256:abc", 1_048_576));
        assert!(tracker.piece_available("sha256:abc", 2_097_152));
        assert!(!tracker.piece_available("sha256:abc", 3_145_728)); // beyond layer
        assert_eq!(tracker.piece_count("sha256:abc"), 3);
    }

    #[tokio::test]
    async fn test_piece_tracker_peers_for_layer() {
        let tracker = PieceTracker::new();

        tracker.register_piece("sha256:abc", 0, &PeerId("n1".to_string()));
        tracker.register_piece("sha256:abc", 1_048_576, &PeerId("n2".to_string()));
        tracker.register_piece("sha256:abc", 1_048_576, &PeerId("n3".to_string()));

        let peers = tracker.peers_for_layer("sha256:abc");
        assert_eq!(peers.len(), 3);
    }

    #[test]
    fn test_scheduler_basic() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let peer = make_peer("node-1", 31415, 100_000_000);
        peers.insert(peer.id.clone(), peer);

        // Register a 2MB layer (2 pieces) from node-1
        tracker.register_layer("sha256:abc", 2_000_000, &PeerId("node-1".to_string()));

        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let assignments = scheduler.schedule("sha256:abc", 2_000_000);

        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].peer_id.0, "node-1");
        assert_eq!(assignments[1].peer_id.0, "node-1");
    }

    #[test]
    fn test_scheduler_no_pieces_available() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let scheduler = PieceScheduler::new(tracker, peers, 16);
        let assignments = scheduler.schedule("sha256:abc", 1_000_000);

        assert!(assignments.is_empty());
    }

    #[test]
    fn test_scheduler_rarest_first() {
        let tracker = Arc::new(PieceTracker::new());
        let peers = Arc::new(DashMap::new());

        let p1 = make_peer("rare-peer", 31415, 50_000_000);
        let p2 = make_peer("common-peer", 31416, 100_000_000);
        peers.insert(p1.id.clone(), p1);
        peers.insert(p2.id.clone(), p2);

        // Piece 0: only rare-peer has it (rare piece)
        tracker.register_piece("sha256:abc", 0, &PeerId("rare-peer".to_string()));
        // Piece 1: both have it (common piece)
        tracker.register_piece("sha256:abc", 1_048_576, &PeerId("rare-peer".to_string()));
        tracker.register_piece("sha256:abc", 1_048_576, &PeerId("common-peer".to_string()));

        let scheduler = PieceScheduler::new(tracker.clone(), peers.clone(), 16);
        let assignments = scheduler.schedule("sha256:abc", 2_097_152);

        assert_eq!(assignments.len(), 2);
        // Rarest piece (offset 0) should be scheduled first
        assert_eq!(assignments[0].offset, 0);
        assert_eq!(assignments[0].peer_id.0, "rare-peer");
    }

    #[tokio::test]
    async fn test_discovery_add_remove_peer() {
        use shellwego_registry::p2p::discovery::PeerDiscovery;

        let peers = Arc::new(DashMap::new());
        let tracker = Arc::new(PieceTracker::new());
        let discovery = PeerDiscovery::new(peers.clone(), tracker.clone()).unwrap();

        assert_eq!(discovery.peer_count(), 0);

        let peer = make_peer("node-1", 31415, 100_000_000);
        discovery.add_peer(peer);
        assert_eq!(discovery.peer_count(), 1);
        assert!(discovery.has_peer(&PeerId("node-1".to_string())));

        discovery.remove_peer(&PeerId("node-1".to_string()));
        assert_eq!(discovery.peer_count(), 0);
        assert!(!discovery.has_peer(&PeerId("node-1".to_string())));
    }

    #[tokio::test]
    async fn test_dragonfly_client_no_peers_returns_none() {
        use shellwego_registry::p2p::client::DragonflyClient;

        let client = DragonflyClient::new(1).await.unwrap();
        let result = client.pull_layer("sha256:abc123", 1_000_000).await.unwrap();

        // Should return None when no peers are available
        assert!(result.is_none());
    }
}
