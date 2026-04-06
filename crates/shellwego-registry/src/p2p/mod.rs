//! P2P image distribution (Dragonfly-inspired)
//!
//! Implements a peer-to-peer image layer distribution system where edge nodes
//! can share container image layers without each pulling independently from the
//! upstream registry. Inspired by the Dragonfly P2P file distribution system.
//!
//! ## Architecture
//!
//! ```text
//!   Node A ──→ Node B ──→ Node C
//!     ↑          ↑          ↑
//!     └──────────┴──────────┘
//!          P2P Layer Sharing
//! ```
//!
//! ## Module Organization
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `peer` | Peer identification and information |
//! | `discovery` | Peer discovery (control-plane gossip, mDNS) |
//! | `piece` | Piece-level availability tracking (1MB chunks) |
//! | `scheduler` | Rarest-first piece scheduling |
//! | `transport` | HTTP-based piece transfer |
//! | `client` | High-level Dragonfly client |

pub mod client;
pub mod discovery;
pub mod peer;
pub mod piece;
pub mod scheduler;
pub mod transport;

// Re-export key types
pub use client::DragonflyClient;
pub use peer::{PeerId, PeerInfo};
pub use piece::PieceTracker;
pub use scheduler::PieceScheduler;
