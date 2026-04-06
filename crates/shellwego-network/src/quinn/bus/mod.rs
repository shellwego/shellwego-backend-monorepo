//! QUIC Message Bus
//!
//! A real publish/subscribe message bus built on top of the Quinn QUIC
//! client/server foundation. Provides topic-based routing, fan-out delivery,
//! at-least-once delivery guarantees, backpressure, and dead-letter queuing.
//!
//! ## Architecture
//!
//! ```text
//! Publisher → BusMessage → BusRouter.publish() → [matching subscribers] → inbox channels
//!
//! Subscribers register via BusRouter.subscribe() with a topic pattern.
//! Patterns support wildcards: `agent.*` (single segment), `node.>` (multi-level).
//! ```

pub mod topic;
pub mod envelope;
pub mod router;
pub mod reliability;

#[cfg(test)]
pub mod bench;
