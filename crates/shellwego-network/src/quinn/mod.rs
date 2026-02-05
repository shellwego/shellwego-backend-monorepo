//! QUIC-based communication layer for Control Plane <-> Agent
//! 
//! This module provides a zero-dependency alternative to NATS for
//! secure, multiplexed communication between the control plane and agents.

pub mod common;
pub mod client;
pub mod server;

pub use server::{QuinnServer, AgentConnection};
pub use client::QuinnClient;
pub use common::{Message, QuicConfig};
