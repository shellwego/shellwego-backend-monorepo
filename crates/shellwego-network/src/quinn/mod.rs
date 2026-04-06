//! QUIC-based communication layer for Control Plane <-> Agent
//!
//! This module provides a zero-dependency alternative to NATS for
//! secure, multiplexed communication between the control plane and agents.
//!
//! ## Modules
//!
//! - `common` — Re-exports of QUIC types from the schema crate.
//! - `client` — QUIC client for connecting to a ShellWeGo server.
//! - `server` — QUIC server for accepting agent connections.
//! - `bus` — QUIC Message Bus (pub/sub routing, reliability, benchmarks).

pub mod common;
pub mod client;
pub mod server;
pub mod bus;

// Re-export QuinnServer and AgentConn from server
pub use server::{QuinnServer, AgentConn};
// Re-export QuinnClient from client
pub use client::QuinnClient;
// Re-export types from common (which re-exports from schema)
pub use common::{Message, QuicConfig, ResourceLimits, ChannelPriority};

// Re-export bus types
pub use bus::router::{BusRouter, BusMetrics, BusError, DeadLetterEntry};
pub use bus::topic::Topic;
pub use bus::reliability::{ReliabilityLayer, ReliabilityMetrics, MessageDedup};
