//! Re-export QUIC types from schema crate
//!
//! Types are defined in shellwego-schema and re-exported here for backward compatibility.

// Re-export types from schema crate
pub use shellwego_schema::{Message, QuicConfig, ResourceLimits, ChannelPriority};
