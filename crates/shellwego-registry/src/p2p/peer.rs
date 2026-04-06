//! Peer identification and information.
//!
//! Each node in the P2P network is identified by a unique `PeerId`.
//! `PeerInfo` tracks the peer's address, available layers, bandwidth,
//! and connection state.

use std::net::SocketAddr;
use std::time::Instant;

/// Unique peer identifier.
///
/// In Phase 1, this is a custom string-based ID.
/// In Phase 2, this could be a libp2p `PeerId`.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PeerId(pub String);

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for PeerId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for PeerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(PeerId(s))
    }
}

/// Information about a peer in the P2P network.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Unique peer identifier
    pub id: PeerId,
    /// Network address (IP:port) for piece transfer
    pub address: SocketAddr,
    /// Set of layer digests this peer has fully available
    pub available_layers: Vec<String>,
    /// Estimated upload bandwidth in bytes/sec
    pub bandwidth_bps: u64,
    /// Round-trip time in milliseconds
    pub rtt_ms: u32,
    /// Maximum concurrent piece downloads from this peer
    pub max_concurrent: u32,
    /// Current concurrent downloads from this peer
    pub current_downloads: u32,
    /// Last successful communication timestamp
    pub last_seen: Instant,
}

impl PeerInfo {
    /// Create a new peer info.
    pub fn new(id: PeerId, address: SocketAddr) -> Self {
        Self {
            id,
            address,
            available_layers: Vec::new(),
            bandwidth_bps: 100_000_000, // 100 MB/s default
            rtt_ms: 10,
            max_concurrent: 4,
            current_downloads: 0,
            last_seen: Instant::now(),
        }
    }

    /// Check if a peer is stale (hasn't been seen recently).
    pub fn is_stale(&self, timeout: std::time::Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }

    /// Check if the peer has capacity for more concurrent downloads.
    pub fn has_capacity(&self) -> bool {
        self.current_downloads < self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[test]
    fn test_peer_id_display() {
        let id = PeerId("node-1".to_string());
        assert_eq!(format!("{}", id), "node-1");
    }

    #[test]
    fn test_peer_id_equality() {
        let a = PeerId("node-1".to_string());
        let b = PeerId("node-1".to_string());
        let c = PeerId("node-2".to_string());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_peer_info_capacity() {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 31415));
        let peer = PeerInfo::new(PeerId("node-1".to_string()), addr);
        assert!(peer.has_capacity());

        let mut peer_no_cap = peer.clone();
        peer_no_cap.current_downloads = peer_no_cap.max_concurrent;
        assert!(!peer_no_cap.has_capacity());
    }

    #[test]
    fn test_peer_info_stale() {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 31415));
        let mut peer = PeerInfo::new(PeerId("node-1".to_string()), addr);
        assert!(!peer.is_stale(std::time::Duration::from_secs(60)));

        peer.last_seen = Instant::now() - std::time::Duration::from_secs(120);
        assert!(peer.is_stale(std::time::Duration::from_secs(60)));
    }
}
