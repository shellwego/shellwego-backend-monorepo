//! QUIC-based communication layer for Control Plane <-> Agent
//! 
//! This module provides a zero-dependency alternative to NATS for
//! secure, multiplexed communication between the control plane and agents.

pub mod common;
pub mod client;
pub mod server;

// Re-export QuinnServer and AgentConn from server
pub use server::{QuinnServer, AgentConn};
// Re-export QuinnClient from client
pub use client::QuinnClient;
// Re-export types from common (which re-exports from schema)
pub use common::{Message, QuicConfig, ResourceLimits, ChannelPriority};
