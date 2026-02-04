//! QUIC-based communication layer for Control Plane <-> Agent
//! 
//! This module provides a zero-dependency alternative to NATS for
//! secure, multiplexed communication between the control plane and agents.
//! 
//! # Architecture
//! 
//! - [`QuinnClient`] - Client for agents to connect to control plane
//! - [`QuinnServer`] - Server for control plane to accept agent connections
//! - [`Message`] - Common message types for CP<->Agent communication
//! 
//! # Example
//! 
//! ```ignore
//! // Agent side
//! let client = QuinnClient::connect("wss://control-plane.example.com").await?;
//! client.send(Message::Heartbeat { node_id }).await?;
//! 
//! // Control plane side  
//! let server = QuinnServer::bind("0.0.0.0:443").await?;
//! while let Some(stream) = server.accept().await {
//!     handle_agent_stream(stream).await;
//! }
//! ```

pub mod common;
pub mod client;
pub mod server;
