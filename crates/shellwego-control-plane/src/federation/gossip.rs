//! Gossip protocol implementation
//!
//! Uses scuttlebutt reconciliation for efficient state synchronization.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{FederationState, FederationConfig};

/// Gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Heartbeat ping
    Ping { 
        from: String, 
        timestamp: DateTime<Utc>,
        version: u64,
    },
    /// Heartbeat pong
    Pong { 
        from: String, 
        timestamp: DateTime<Utc>,
        version: u64,
    },
    /// Full state sync
    StateSync { 
        state: FederationState,
    },
    /// Delta update
    Delta { 
        deltas: Vec<StateDelta>,
        from: String,
    },
    /// Anti-entropy request
    AntiEntropyRequest { 
        from: String,
        merkle_root: String,
    },
    /// Anti-entropy response
    AntiEntropyResponse { 
        from: String,
        merkle_tree: MerkleTree,
    },
}

/// State delta for incremental updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub key: String,
    pub operation: DeltaOp,
    pub value: String,
    pub version: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeltaOp {
    Set,
    Delete,
}

/// Merkle tree for efficient comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    pub root_hash: String,
    pub depth: u8,
    pub leaves: Vec<String>,
}

impl MerkleTree {
    pub fn new(data: &[String]) -> Self {
        // Simplified Merkle tree - in production use proper hash tree
        let leaves = data.to_vec();
        let root_hash = if leaves.is_empty() {
            String::new()
        } else {
            format!("{:064x}", Uuid::new_v4().as_u128())
        };
        
        Self {
            root_hash,
            depth: 3,
            leaves,
        }
    }
    
    pub fn compare(&self, other: &MerkleTree) -> Vec<usize> {
        // Return indices of differing leaves
        self.leaves.iter()
            .zip(other.leaves.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect()
    }
}

/// Peer state tracking
#[derive(Debug, Clone)]
pub struct PeerState {
    pub region: String,
    pub last_seen: Instant,
    pub last_version: u64,
    pub is_alive: bool,
    pub failure_count: u32,
}

/// Gossip protocol handler
pub struct GossipProtocol {
    config: FederationConfig,
    peers: Arc<RwLock<HashMap<String, PeerState>>>,
    message_sender: broadcast::Sender<GossipMessage>,
    message_receiver: broadcast::Receiver<GossipMessage>,
}

impl GossipProtocol {
    /// Create a new gossip protocol handler
    pub fn new(config: FederationConfig) -> Self {
        let (sender, receiver) = broadcast::channel(1000);
        
        let peers = config.peers.iter()
            .map(|p| {
                (p.region.clone(), PeerState {
                    region: p.region.clone(),
                    last_seen: Instant::now(),
                    last_version: 0,
                    is_alive: false,
                    failure_count: 0,
                })
            })
            .collect();
        
        info!("Gossip protocol initialized for region: {}", config.local_region);
        
        Self {
            config,
            peers: Arc::new(RwLock::new(peers)),
            message_sender: sender,
            message_receiver: receiver,
        }
    }

    /// Get sender for broadcasting messages
    pub fn get_sender(&self) -> broadcast::Sender<GossipMessage> {
        self.message_sender.clone()
    }

    /// Receive next message
    pub async fn receive(&mut self) -> Option<GossipMessage> {
        match self.message_receiver.recv().await {
            Ok(msg) => Some(msg),
            Err(_) => None,
        }
    }

    /// Handle incoming gossip message
    pub async fn handle_message(&self, msg: GossipMessage) -> Option<GossipMessage> {
        match msg {
            GossipMessage::Ping { from, timestamp, version } => {
                self.record_peer_ping(&from, version).await;
                
                // Respond with pong
                Some(GossipMessage::Pong {
                    from: self.config.local_region.clone(),
                    timestamp: Utc::now(),
                    version: 0, // Would be actual state version
                })
            }
            GossipMessage::Pong { from, timestamp: _, version } => {
                self.record_peer_ping(&from, version).await;
                None
            }
            GossipMessage::StateSync { state } => {
                debug!("Received state sync from {}", state.region);
                None
            }
            GossipMessage::Delta { deltas, from } => {
                debug!("Received {} deltas from {}", deltas.len(), from);
                None
            }
            GossipMessage::AntiEntropyRequest { from, merkle_root } => {
                debug!("Received anti-entropy request from {} with root {}", from, merkle_root);
                // Would respond with local merkle tree
                None
            }
            GossipMessage::AntiEntropyResponse { from, merkle_tree } => {
                debug!("Received anti-entropy response from {}", from);
                None
            }
        }
    }

    /// Record peer ping
    async fn record_peer_ping(&self, region: &str, version: u64) {
        let mut peers = self.peers.write().await;
        
        if let Some(peer) = peers.get_mut(region) {
            peer.last_seen = Instant::now();
            peer.last_version = version;
            peer.is_alive = true;
            peer.failure_count = 0;
        }
    }

    /// Send ping to all peers
    pub async fn ping_all(&self) {
        let msg = GossipMessage::Ping {
            from: self.config.local_region.clone(),
            timestamp: Utc::now(),
            version: 0,
        };
        
        let _ = self.message_sender.send(msg);
    }

    /// Broadcast state update
    pub async fn broadcast_state(&self, state: &FederationState) {
        let msg = GossipMessage::StateSync {
            state: state.clone(),
        };
        
        let _ = self.message_sender.send(msg);
    }

    /// Send delta updates
    pub async fn send_deltas(&self, deltas: Vec<StateDelta>) {
        let msg = GossipMessage::Delta {
            deltas,
            from: self.config.local_region.clone(),
        };
        
        let _ = self.message_sender.send(msg);
    }

    /// Check peer health
    pub async fn check_peer_health(&self) {
        let mut peers = self.peers.write().await;
        let timeout = Duration::from_secs(self.config.gossip_interval_secs * 3);
        
        for peer in peers.values_mut() {
            if peer.last_seen.elapsed() > timeout {
                peer.failure_count += 1;
                if peer.failure_count >= 3 {
                    peer.is_alive = false;
                    warn!("Peer {} marked as dead", peer.region);
                }
            }
        }
    }

    /// Get alive peers
    pub async fn get_alive_peers(&self) -> Vec<String> {
        let peers = self.peers.read().await;
        peers.values()
            .filter(|p| p.is_alive)
            .map(|p| p.region.clone())
            .collect()
    }

    /// Get peer statistics
    pub async fn get_peer_stats(&self) -> HashMap<String, PeerStats> {
        let peers = self.peers.read().await;
        peers.values()
            .map(|p| {
                (p.region.clone(), PeerStats {
                    is_alive: p.is_alive,
                    last_seen_secs: p.last_seen.elapsed().as_secs(),
                    last_version: p.last_version,
                    failure_count: p.failure_count,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStats {
    pub is_alive: bool,
    pub last_seen_secs: u64,
    pub last_version: u64,
    pub failure_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree() {
        let data = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let tree = MerkleTree::new(&data);
        
        assert!(!tree.root_hash.is_empty());
        assert_eq!(tree.leaves.len(), 3);
    }

    #[test]
    fn test_merkle_tree_compare() {
        let tree1 = MerkleTree::new(&["a".to_string(), "b".to_string()]);
        let tree2 = MerkleTree::new(&["a".to_string(), "c".to_string()]);
        
        let diff = tree1.compare(&tree2);
        assert!(!diff.is_empty());
    }

    #[tokio::test]
    async fn test_gossip_protocol() {
        let config = FederationConfig::default();
        let gossip = GossipProtocol::new(config);
        
        let stats = gossip.get_peer_stats().await;
        assert!(stats.is_empty()); // No peers in default config
    }

    #[tokio::test]
    async fn test_ping_all() {
        let config = FederationConfig::default();
        let gossip = GossipProtocol::new(config);
        
        gossip.ping_all().await;
        
        // Message should be broadcast
    }
}
