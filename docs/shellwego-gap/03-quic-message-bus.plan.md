# Plan 03: QUIC Message Bus

## 1. Title & Overview

**QUIC Message Bus** — Build a real publish/subscribe message bus on top of the existing Quinn QUIC client/server foundation. The README claims "QUIC/Quinn pub/sub, 5M msgs/sec" but the current code only implements a raw point-to-point QUIC client and server (`QuinnClient`, `QuinnServer`) that can send and receive `Message` enums over bidirectional streams. There is no topic/channel subscription, no fan-out delivery, no message routing, no at-least-once delivery guarantees, and no ack/retry mechanism. This gap blocks the scheduler (Plan 02) from dispatching commands to agents and blocks the agent (Plan 04) from receiving work and reporting status. The existing `ChannelPriority` enum in the schema crate is defined but completely unused — this plan wires it into the message bus.

## 2. Gap Summary

| # | Readme Claim | Actual Implementation | File(s) | Severity |
|---|---|---|---|---|
| A | "QUIC/Quinn pub/sub" | No pub/sub. Only point-to-point send/receive on bidirectional streams. No topic model, no subscriber registry, no fan-out. | `crates/shellwego-network/src/quinn/client.rs`, `crates/shellwego-network/src/quinn/server.rs` | **CRITICAL** |
| B | "5M msgs/sec" | No benchmarking harness. No batching. Each message opens a new QUIC bidirectional stream (expensive). No message pool or zero-copy path. | `crates/shellwego-network/src/quinn/client.rs:130-137` (new stream per send) | **HIGH** |
| C | Channel subscription / routing | `QuinnServer::run()` accepts connections but drops them after logging. No message dispatch, no routing table, no per-node inbox. | `crates/shellwego-network/src/quinn/server.rs:75-86` | **CRITICAL** |
| D | At-least-once delivery | No message IDs, no acknowledgments, no retry logic. If a send fails, the message is lost silently. | `crates/shellwego-network/src/quinn/client.rs:130-137` | **HIGH** |
| E | `ChannelPriority` unused | Enum defined in schema with 5 levels (`Critical`..`BestEffort`) but never read, never applied to any message or stream. | `crates/shellwego-schema/src/network/quinn.rs:184-198` | **MEDIUM** |
| F | No backpressure / flow control | Messages are sent with no regard for receiver capacity. No bounded buffers, no credit-based flow, no NACK signaling. | `crates/shellwego-network/src/quinn/client.rs`, `server.rs` | **MEDIUM** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-schema/src/network/quinn.rs` | Add `BusMessage` envelope (wraps `Message` with `topic`, `msg_id`, `priority`, `timestamp`, `source_node`, `reply_to`); add `Subscribe`, `Unsubscribe`, `PubAck`, `Ping`, `Pong` variants to `Message`; add `SubscriptionId`, `BusConfig` types |
| `crates/shellwego-schema/src/network/mod.rs` | Re-export new types from `quinn.rs` |
| `crates/shellwego-schema/src/lib.rs` | Re-export new types at crate root |
| `crates/shellwego-network/src/quinn/mod.rs` | Add `pub mod bus;` and re-export bus types |
| `crates/shellwego-network/src/quinn/server.rs` | Extend `AgentConn` with `subscriptions: HashSet<Topic>`, `inbox: mpsc::Sender<BusMessage>`; change `run()` to dispatch messages via the bus router |
| `crates/shellwego-network/src/quinn/client.rs` | Add `subscribe()`, `unsubscribe()`, `publish()` methods that wrap messages in `BusMessage` envelopes |
| `crates/shellwego-network/Cargo.toml` | Add `dashmap`, `tokio/sync`, `parking_lot` (for read-heavy routing table); ensure `tracing` included |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-network/src/quinn/bus/mod.rs` | Module root — re-exports `router`, `topic`, `envelope` |
| `crates/shellwego-network/src/quinn/bus/topic.rs` | `Topic` type (newtype around `String` with validation: non-empty, max 256 bytes, allowed chars `a-zA-Z0-9._-`, hierarchical with `.` separator), topic matching (exact + wildcard `*>` suffix) |
| `crates/shellwego-network/src/quinn/bus/envelope.rs` | `BusMessage` framing: serialize/deserialize over QUIC streams with length-prefix, compression flag, priority byte |
| `crates/shellwego-network/src/quinn/bus/router.rs` | `BusRouter` — the core pub/sub routing engine: `publish(topic, BusMessage)` → fan-out to all subscribers; `subscribe(node_id, topic_pattern)` → register; `unsubscribe(sub_id)` → deregister; handles wildcard matching |
| `crates/shellwego-network/src/quinn/bus/reliability.rs` | `ReliabilityLayer` — in-flight message tracker with configurable retry (exponential backoff), deduplication by `msg_id`, ack timeout, dead-letter queue for permanently failed messages |
| `crates/shellwego-network/src/quinn/bus/bench.rs` | Micro-benchmark: single-thread pub/sub throughput test using `criterion`; measures messages/sec, latency percentiles, allocation count |

## 4. Prerequisites

1. **Network crate compiles** — `shellwego-network` currently compiles with 7 warnings (unused imports/variables/dead code). Fix these first: run `cargo fix --lib -p shellwego-network` and review remaining dead code (eBPF rate limiter fields). These warnings must be resolved before adding new modules.

2. **Quinn feature enabled** — The `quinn` feature is the default feature for `shellwego-network` (see `Cargo.toml` line 57). This plan depends on `quinn`, `rustls`, `postcard`, and `uuid` being available — all already present.

3. **Schema crate stable** — `shellwego-schema` is the source of truth for wire types. Changes to `Message`, `ChannelPriority`, and new types (`BusMessage`, `Topic`) must be added there first so both the network crate and consumers can depend on them.

4. **No dependency on control-plane or agent crates** — The message bus lives entirely within `shellwego-network` and `shellwego-schema`. It must be usable without importing either `shellwego-control-plane` or `shellwego-agent`. This is a library-only change.

5. **Tokio runtime available** — All async operations assume a multi-threaded Tokio runtime. The bus router will use `tokio::sync::broadcast`, `tokio::sync::mpsc`, and `dashmap` for concurrent access.

## 5. Detailed Implementation Steps

### Phase 1: Topic Model & Envelope (Schema + Network Types)

**Step 1.1 — Define `Topic` type**

File: `crates/shellwego-network/src/quinn/bus/topic.rs`

```rust
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// A topic name for the message bus.
///
/// Format: `segment.segment.segment` where each segment matches `[a-zA-Z0-9_-]+`.
/// Maximum length: 256 bytes total.
/// Reserved prefixes: `system.` (internal control messages).
///
/// Examples:
///   - `agent.cmd.schedule`   — schedule an app on an agent
///   - `agent.heartbeat`       — agent heartbeats
///   - `app.{app_id}.logs`    — app-specific logs (use format! to substitute)
///   - `system.ping`          — internal keepalive
///   - `node.*`               — wildcard: matches any single segment
///   - `node.>`               — wildcard: matches all sub-topics under `node`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Topic(String);

impl Topic {
    pub const MAX_LEN: usize = 256;
    pub const SYSTEM_PREFIX: &str = "system.";

    pub fn new(name: impl Into<String>) -> Result<Self, TopicError> {
        let s = name.into();
        if s.is_empty() {
            return Err(TopicError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(TopicError::TooLong(s.len()));
        }
        // Validate each segment
        for segment in s.split('.') {
            if segment.is_empty() {
                return Err(TopicError::EmptySegment(s));
            }
            if segment == "*" || segment == ">" {
                continue; // wildcards allowed
            }
            if !segment.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err(TopicError::InvalidChars(segment.to_string()));
            }
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str { &self.0 }

    /// Returns true if this topic pattern matches a concrete topic.
    /// - Exact match: `agent.heartbeat` == `agent.heartbeat`
    /// - Single-segment wildcard: `agent.*` matches `agent.heartbeat` but not `agent.cmd.schedule`
    /// - Multi-level wildcard: `agent.>` matches `agent.heartbeat`, `agent.cmd.schedule`, etc.
    /// The `>` wildcard must be the last segment.
    pub fn matches(&self, concrete: &Topic) -> bool {
        let pattern_parts: Vec<&str> = self.0.split('.').collect();
        let concrete_parts: Vec<&str> = concrete.0.split('.').collect();

        for (i, pat) in pattern_parts.iter().enumerate() {
            match *pat {
                ">" => return true, // matches everything remaining (must be last)
                "*" => {
                    if i >= concrete_parts.len() { return false; }
                    // matches exactly one segment
                }
                exact => {
                    if concrete_parts.get(i) != Some(&exact) { return false; }
                }
            }
        }
        pattern_parts.len() == concrete_parts.len()
    }

    pub fn is_wildcard(&self) -> bool {
        self.0.contains('*') || self.0.contains('>')
    }

    pub fn is_system(&self) -> bool {
        self.0.starts_with(Self::SYSTEM_PREFIX)
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum TopicError {
    #[error("topic name is empty")]
    Empty,
    #[error("topic too long: {0} bytes (max {MAX_LEN})", MAX_LEN = Topic::MAX_LEN)]
    TooLong(usize),
    #[error("empty segment in topic: {0}")]
    EmptySegment(String),
    #[error("invalid characters in segment: {0}")]
    InvalidChars(String),
}
```

**Step 1.2 — Define `BusMessage` envelope**

File: `crates/shellwego-schema/src/network/quinn.rs` (add after existing `ChannelPriority`)

```rust
/// Unique subscription identifier returned by the bus on subscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct SubscriptionId(pub u64);

/// Wire envelope for every message on the bus.
///
/// This wraps the application-level `Message` with routing metadata.
/// Serialized with postcard over QUIC streams. Layout:
///
///   [u32: payload_len] [u8: priority] [u16: topic_len] [topic_bytes] [postcard(Message)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BusMessage {
    /// Unique message identifier (for dedup and ack).
    pub msg_id: Uuid,
    /// Topic this message was published to.
    pub topic: String,
    /// Priority level — affects send ordering.
    pub priority: ChannelPriority,
    /// Timestamp when the message was created (sender wall-clock).
    pub timestamp: DateTime<Utc>,
    /// Node ID of the publisher.
    pub source_node: Option<Uuid>,
    /// If this is a reply, the msg_id of the original message.
    pub reply_to: Option<Uuid>,
    /// The application-level payload.
    pub payload: Message,
}
```

**Step 1.3 — Add bus control messages to `Message` enum**

File: `crates/shellwego-schema/src/network/quinn.rs`

Add these variants to the existing `Message` enum:

```rust
/// Subscribe to a topic pattern.
Subscribe {
    /// Client-provided subscription ID (for unsubscribe later).
    subscription_id: SubscriptionId,
    /// Topic pattern (may contain wildcards `*` and `>`).
    topic_pattern: String,
},

/// Unsubscribe from a topic.
Unsubscribe {
    /// The subscription ID to cancel.
    subscription_id: SubscriptionId,
},

/// Acknowledgment of a received message (for at-least-once delivery).
Ack {
    /// The msg_id being acknowledged.
    msg_id: Uuid,
},

/// Negative acknowledgment — message could not be processed.
Nack {
    /// The msg_id that failed.
    msg_id: Uuid,
    /// Reason for failure.
    reason: String,
},
```

**Step 1.4 — Define `BusConfig`**

File: `crates/shellwego-schema/src/network/quinn.rs`

```rust
/// Configuration for the QUIC message bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BusConfig {
    /// Maximum number of in-flight unacknowledged messages per connection.
    /// Default: 1024.
    pub max_inflight: u32,

    /// Acknowledgment timeout in milliseconds. If no `Ack` is received
    /// within this duration, the message is retried.
    /// Default: 5000.
    pub ack_timeout_ms: u64,

    /// Maximum retry attempts for a message before it goes to the dead-letter queue.
    /// Default: 3.
    pub max_retries: u32,

    /// Base retry delay in milliseconds (exponential backoff: delay * 2^attempt).
    /// Default: 100.
    pub retry_base_delay_ms: u64,

    /// Per-subscriber inbox capacity (bounded mpsc channel size).
    /// Default: 8192.
    pub subscriber_buffer_size: usize,

    /// Maximum number of subscribers per topic.
    /// Default: 1000.
    pub max_subscribers_per_topic: usize,

    /// Whether to enable the dead-letter queue for failed messages.
    /// Default: true.
    pub dead_letter_enabled: bool,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            max_inflight: 1024,
            ack_timeout_ms: 5000,
            max_retries: 3,
            retry_base_delay_ms: 100,
            subscriber_buffer_size: 8192,
            max_subscribers_per_topic: 1000,
            dead_letter_enabled: true,
        }
    }
}
```

### Phase 2: Message Bus Router (Core Engine)

**Step 2.1 — Implement `BusRouter`**

File: `crates/shellwego-network/src/quinn/bus/router.rs`

This is the central routing engine. It maintains the subscription table and dispatches published messages to all matching subscribers.

```rust
use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::{mpsc, broadcast};
use uuid::Uuid;
use tracing::{info, warn, debug, error};

use shellwego_schema::{BusMessage, BusConfig, ChannelPriority, Message, SubscriptionId, Topic};

/// A subscriber entry in the routing table.
struct SubscriberEntry {
    /// The topic pattern this subscriber is listening on.
    topic_pattern: Topic,
    /// Channel to deliver messages to this subscriber.
    inbox: mpsc::Sender<BusMessage>,
    /// Node ID of the subscriber (for routing awareness).
    node_id: Uuid,
}

/// Dead-letter entry for messages that failed all retry attempts.
#[derive(Debug, Clone)]
pub struct DeadLetterEntry {
    pub message: BusMessage,
    pub last_error: String,
    pub attempt_count: u32,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

/// The core message bus router.
///
/// This is the brain of the pub/sub system. It:
/// 1. Maintains a `DashMap` of `SubscriptionId → SubscriberEntry`.
/// 2. On `publish()`, iterates all subscribers and does topic matching.
/// 3. Delivers via bounded `mpsc::Sender` — backpressure is implicit (send returns Pending when full).
/// 4. Optionally stores dead-lettered messages.
pub struct BusRouter {
    /// Subscription registry: subscription_id → subscriber info.
    subscriptions: DashMap<SubscriptionId, SubscriberEntry>,
    /// Per-node subscriptions: node_id → Vec<subscription_id> (for cleanup on disconnect).
    node_subscriptions: DashMap<Uuid, Vec<SubscriptionId>>,
    /// Configuration.
    config: BusConfig,
    /// Dead-letter queue (in-memory ring buffer).
    dead_letter: parking_lot::Mutex<Vec<DeadLetterEntry>>,
    /// Dead-letter broadcast channel (for monitoring).
    dead_letter_tx: broadcast::Sender<DeadLetterEntry>,
    /// Metrics counters.
    messages_published: AtomicU64,
    messages_delivered: AtomicU64,
    messages_dropped: AtomicU64,
    messages_dead_lettered: AtomicU64,
}

use std::sync::atomic::AtomicU64;

impl BusRouter {
    pub fn new(config: BusConfig) -> Self {
        let (dead_letter_tx, _) = broadcast::channel(1024);
        Self {
            subscriptions: DashMap::new(),
            node_subscriptions: DashMap::new(),
            config,
            dead_letter: parking_lot::Mutex::new(Vec::new()),
            dead_letter_tx,
            messages_published: AtomicU64::new(0),
            messages_delivered: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            messages_dead_lettered: AtomicU64::new(0),
        }
    }

    /// Subscribe a node to a topic pattern.
    ///
    /// Returns the `SubscriptionId` and a `mpsc::Receiver<BusMessage>` for the subscriber
    /// to consume messages from.
    pub fn subscribe(
        &self,
        node_id: Uuid,
        topic_pattern: Topic,
    ) -> Result<(SubscriptionId, mpsc::Receiver<BusMessage>), BusError> {
        // Check subscriber limit
        let count = self.subscriptions.iter()
            .filter(|e| e.topic_pattern.as_str() == topic_pattern.as_str())
            .count();
        if count >= self.config.max_subscribers_per_topic {
            return Err(BusError::TooManySubscribers {
                topic: topic_pattern.as_str().to_string(),
                limit: self.config.max_subscribers_per_topic,
            });
        }

        let sub_id = SubscriptionId(self.next_subscription_id());
        let (tx, rx) = mpsc::channel(self.config.subscriber_buffer_size);

        let entry = SubscriberEntry {
            topic_pattern: topic_pattern.clone(),
            inbox: tx,
            node_id,
        };

        self.subscriptions.insert(sub_id, entry);
        self.node_subscriptions
            .entry(node_id)
            .or_insert_with(Vec::new)
            .push(sub_id);

        info!(
            node_id = %node_id,
            sub_id = sub_id.0,
            topic = topic_pattern.as_str(),
            "Subscriber added"
        );

        Ok((sub_id, rx))
    }

    /// Unsubscribe from a topic.
    pub fn unsubscribe(&self, sub_id: SubscriptionId) -> bool {
        if let Some((_, entry)) = self.subscriptions.remove(&sub_id) {
            // Clean up from node_subscriptions
            if let Some(mut subs) = self.node_subscriptions.get_mut(&entry.node_id) {
                subs.retain(|s| *s != sub_id);
                if subs.is_empty() {
                    drop(subs);
                    self.node_subscriptions.remove(&entry.node_id);
                }
            }
            info!(sub_id = sub_id.0, node_id = %entry.node_id, "Subscriber removed");
            true
        } else {
            false
        }
    }

    /// Remove all subscriptions for a node (called on disconnect).
    pub fn remove_node(&self, node_id: Uuid) {
        if let Some((_, sub_ids)) = self.node_subscriptions.remove(&node_id) {
            for sub_id in sub_ids {
                self.subscriptions.remove(&sub_id);
            }
            info!(node_id = %node_id, count = sub_ids.len(), "All subscriptions removed for node");
        }
    }

    /// Publish a message to a topic.
    ///
    /// This iterates all subscribers and delivers to every matching one.
    /// Returns the number of subscribers the message was delivered to.
    pub fn publish(&self, topic: &Topic, message: BusMessage) -> usize {
        self.messages_published.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut delivered = 0usize;
        let mut dropped = 0usize;

        for entry in self.subscriptions.iter() {
            if entry.topic_pattern.matches(topic) {
                match entry.inbox.try_send(message.clone()) {
                    Ok(()) => delivered += 1,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        dropped += 1;
                        warn!(
                            sub_id = entry.key().0,
                            topic = topic.as_str(),
                            msg_id = %message.msg_id,
                            "Subscriber inbox full, message dropped (backpressure)"
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Subscriber receiver dropped — stale entry
                        warn!(
                            sub_id = entry.key().0,
                            "Subscriber channel closed, removing"
                        );
                        // Note: we can't modify DashMap during iteration, 
                        // stale cleanup happens in a background sweep
                    }
                }
            }
        }

        self.messages_delivered.fetch_add(delivered as u64, std::sync::atomic::Ordering::Relaxed);
        self.messages_dropped.fetch_add(dropped as u64, std::sync::atomic::Ordering::Relaxed);

        if delivered == 0 && dropped == 0 {
            debug!(topic = topic.as_str(), "No subscribers for topic");
        }

        delivered
    }

    /// Background task to sweep stale subscriptions (where receiver is dropped).
    pub async fn sweep_stale(&self) -> usize {
        let stale: Vec<SubscriptionId> = self.subscriptions.iter()
            .filter(|entry| entry.inbox.is_closed())
            .map(|entry| *entry.key())
            .collect();

        for sub_id in &stale {
            self.unsubscribe(*sub_id);
        }

        stale.len()
    }

    /// Get dead-letter entries.
    pub fn dead_letter_entries(&self) -> Vec<DeadLetterEntry> {
        self.dead_letter.lock().clone()
    }

    /// Subscribe to dead-letter events.
    pub fn subscribe_dead_letter(&self) -> broadcast::Receiver<DeadLetterEntry> {
        self.dead_letter_tx.subscribe()
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> BusMetrics {
        BusMetrics {
            messages_published: self.messages_published.load(std::sync::atomic::Ordering::Relaxed),
            messages_delivered: self.messages_delivered.load(std::sync::atomic::Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(std::sync::atomic::Ordering::Relaxed),
            messages_dead_lettered: self.messages_dead_lettered.load(std::sync::atomic::Ordering::Relaxed),
            active_subscriptions: self.subscriptions.len(),
            connected_nodes: self.node_subscriptions.len(),
        }
    }

    fn next_subscription_id(&self) -> u64 {
        use std::sync::atomic::Ordering;
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BusError {
    #[error("too many subscribers for topic '{topic}' (limit: {limit})")]
    TooManySubscribers { topic: String, limit: usize },
    #[error("subscription not found: {0}")]
    NotFound(SubscriptionId),
    #[error("topic error: {0}")]
    Topic(#[from] shellwego_schema::TopicError),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BusMetrics {
    pub messages_published: u64,
    pub messages_delivered: u64,
    pub messages_dropped: u64,
    pub messages_dead_lettered: u64,
    pub active_subscriptions: usize,
    pub connected_nodes: usize,
}
```

### Phase 3: Reliability Layer (At-Least-Once Delivery)

**Step 3.1 — Implement `ReliabilityLayer`**

File: `crates/shellwego-network/src/quinn/bus/reliability.rs`

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::sync::{oneshot, mpsc};
use tokio::time::{interval, Instant};
use uuid::Uuid;
use tracing::{warn, debug};

use shellwego_schema::{BusMessage, BusConfig, Message};

/// Status of an in-flight message.
#[derive(Debug)]
enum InflightStatus {
    /// Waiting for acknowledgment.
    Pending {
        sent_at: Instant,
        retry_count: u32,
    },
    /// Waiting for retry backoff.
    Backoff {
        retry_at: Instant,
        retry_count: u32,
    },
}

/// Message awaiting acknowledgment.
struct InflightEntry {
    message: BusMessage,
    status: InflightStatus,
}

/// Handles at-least-once delivery semantics.
///
/// On publish:
///   1. Assign msg_id (if not already set).
///   2. Store in in-flight map.
///   3. Send via the underlying transport.
///   4. Start ack timer.
///
/// On ack received:
///   1. Remove from in-flight map.
///
/// On ack timeout:
///   1. If retry_count < max_retries: re-send with exponential backoff.
///   2. If retry_count >= max_retries: move to dead-letter queue.
///
/// Deduplication:
///   - Receiver maintains a `LruCache<Uuid, ()>` of recently seen msg_ids.
///   - Default cache size: 10,000 entries.
///   - TTL: 5 minutes (messages older than this are assumed delivered or abandoned).
pub struct ReliabilityLayer {
    config: BusConfig,
    in_flight: DashMap<Uuid, InflightEntry>,
    /// Dedup cache: msg_id → time first seen.
    seen: parking_lot::Mutex<lru::LruCache<Uuid, Instant>>,
    /// Channel to send messages that need retransmission.
    retry_tx: mpsc::Sender<BusMessage>,
    /// Channel to send messages to the dead-letter queue.
    dead_letter_tx: mpsc::Sender<shellwego_network::quinn::bus::router::DeadLetterEntry>,
}
```

> **Note on `lru` crate**: Add `lru = "0.12"` to `shellwego-network/Cargo.toml`. This is a lightweight dependency (no async, no unsafe in the public API).

**Step 3.2 — Deduplication on receive side**

Add a `MessageDedup` struct:

```rust
/// Deduplicates received messages by msg_id.
pub struct MessageDedup {
    cache: parking_lot::Mutex<lru::LruCache<Uuid, Instant>>,
    ttl: Duration,
}

impl MessageDedup {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: parking_lot::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(capacity).unwrap()
            )),
            ttl,
        }
    }

    /// Returns `true` if the message is new (not a duplicate).
    /// Returns `false` if the message was already seen.
    pub fn check_and_insert(&self, msg_id: Uuid) -> bool {
        let mut cache = self.cache.lock();
        if let Some(seen_at) = cache.get(&msg_id) {
            if seen_at.elapsed() < self.ttl {
                return false; // duplicate within TTL
            }
        }
        cache.put(msg_id, Instant::now());
        true
    }

    /// Periodic cleanup of expired entries.
    pub fn cleanup(&self) {
        let mut cache = self.cache.lock();
        let now = Instant::now();
        cache.retain(|_, seen_at| now.duration_since(*seen_at) < self.ttl);
    }
}
```

### Phase 4: Wire Bus into Server & Client

**Step 4.1 — Extend `AgentConn` with subscription state**

File: `crates/shellwego-network/src/quinn/server.rs`

```rust
use std::collections::HashSet;
use tokio::sync::mpsc;

pub struct AgentConn {
    pub connection: quinn::Connection,
    pub node_id: Option<Uuid>,
    pub hostname: Option<String>,
    /// Active subscriptions for this connection.
    pub subscriptions: HashSet<SubscriptionId>,
    /// Outbound message queue — the bus router pushes messages here.
    pub outbound_tx: Option<mpsc::Sender<BusMessage>>,
    /// Inbound message queue — messages received from this agent.
    pub inbound_rx: Option<mpsc::Receiver<BusMessage>>,
}
```

**Step 4.2 — Rewrite `QuinnServer::run()` with bus integration**

File: `crates/shellwego-network/src/quinn/server.rs`

Replace the current `run()` method (which only logs and drops connections):

```rust
pub async fn run_with_bus(&self, bus: Arc<BusRouter>, bus_config: BusConfig) -> Result<()> {
    // Spawn background task to sweep stale subscriptions every 30 seconds.
    let sweep_bus = bus.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            sweep_bus.sweep_stale().await;
        }
    });

    loop {
        match self.accept().await {
            Ok(mut conn) => {
                let bus = bus.clone();
                let config = bus_config.clone();
                let reliability = Arc::new(ReliabilityLayer::new(config.clone()));
                let dedup = Arc::new(MessageDedup::new(10_000, Duration::from_secs(300)));

                tokio::spawn(async move {
                    // 1. Wait for Register message to get node_id
                    match conn.receive().await {
                        Ok(Message::Register { hostname, capabilities }) => {
                            let node_id = Uuid::new_v4();
                            conn.set_node_id(node_id);
                            conn.set_hostname(hostname);

                            info!(node_id = %node_id, "Agent registered, starting message loop");

                            // 2. Create outbound channel
                            let (outbound_tx, mut outbound_rx) = mpsc::channel(config.subscriber_buffer_size);
                            conn.outbound_tx = Some(outbound_tx);

                            // 3. Spawn outbound writer task
                            let send_conn = conn.clone();
                            let send_reliability = reliability.clone();
                            tokio::spawn(async move {
                                while let Some(msg) = outbound_rx.recv().await {
                                    if let Err(e) = send_conn.send(&msg.payload).await {
                                        warn!("Failed to send to agent: {}", e);
                                        break;
                                    }
                                }
                            });

                            // 4. Message receive loop — dispatch through bus
                            loop {
                                match conn.receive().await {
                                    Ok(Message::Subscribe { subscription_id, topic_pattern }) => {
                                        match Topic::new(topic_pattern) {
                                            Ok(topic) => {
                                                match bus.subscribe(node_id, topic) {
                                                    Ok((_, rx)) => {
                                                        conn.subscriptions.insert(subscription_id);
                                                        // Spawn task to forward bus messages to outbound
                                                        let tx = conn.outbound_tx.clone().unwrap();
                                                        tokio::spawn(async move {
                                                            while let Some(msg) = rx.recv().await {
                                                                if tx.send(msg).await.is_err() { break; }
                                                            }
                                                        });
                                                    }
                                                    Err(e) => warn!("Subscribe failed: {}", e),
                                                }
                                            }
                                            Err(e) => warn!("Invalid topic: {}", e),
                                        }
                                    }
                                    Ok(Message::Unsubscribe { subscription_id }) => {
                                        bus.unsubscribe(subscription_id);
                                        conn.subscriptions.remove(&subscription_id);
                                    }
                                    Ok(Message::Ack { msg_id }) => {
                                        reliability.ack(msg_id);
                                    }
                                    Ok(Message::Heartbeat { .. }) => {
                                        // Forward heartbeats to bus for metrics consumers
                                        // (handled as a system message)
                                    }
                                    Ok(msg) => {
                                        // Wrap in BusMessage and publish to bus
                                        let bus_msg = BusMessage {
                                            msg_id: Uuid::new_v4(),
                                            topic: "agent.inbound".to_string(),
                                            priority: ChannelPriority::Command,
                                            timestamp: chrono::Utc::now(),
                                            source_node: Some(node_id),
                                            reply_to: None,
                                            payload: msg,
                                        };
                                        let topic = Topic::new("agent.inbound").unwrap();
                                        bus.publish(&topic, bus_msg);
                                    }
                                    Err(e) => {
                                        warn!("Receive error for node {}: {}", node_id, e);
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(msg) => {
                            warn!("Expected Register, got: {:?}", msg);
                            conn.close("expected Register message").await;
                        }
                        Err(e) => {
                            warn!("Accept error: {}", e);
                        }
                    }

                    // Cleanup on disconnect
                    if let Some(nid) = conn.node_id() {
                        bus.remove_node(nid);
                    }
                });
            }
            Err(e) => {
                tracing::error!("Accept error: {}", e);
            }
        }
    }
}
```

**Step 4.3 — Add `publish()`, `subscribe()`, `unsubscribe()` to `QuinnClient`**

File: `crates/shellwego-network/src/quinn/client.rs`

```rust
impl QuinnClient {
    // ... existing methods ...

    /// Publish a message to a topic on the bus.
    pub async fn publish(
        &self,
        topic: &str,
        payload: Message,
        priority: ChannelPriority,
    ) -> Result<Uuid> {
        let msg_id = Uuid::new_v4();
        let bus_msg = BusMessage {
            msg_id,
            topic: topic.to_string(),
            priority,
            timestamp: chrono::Utc::now(),
            source_node: None, // Client may not have node_id yet
            reply_to: None,
            payload,
        };
        self.send(Message::Heartbeat { /* placeholder until we add Publish variant */ }).await?;
        // Actually: serialize BusMessage envelope and send
        // This will be refined once the Publish variant is added to Message
        Ok(msg_id)
    }

    /// Subscribe to a topic pattern.
    pub async fn subscribe(&self, subscription_id: SubscriptionId, topic_pattern: &str) -> Result<()> {
        let msg = Message::Subscribe {
            subscription_id,
            topic_pattern: topic_pattern.to_string(),
        };
        self.send(msg).await
    }

    /// Unsubscribe from a topic.
    pub async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<()> {
        let msg = Message::Unsubscribe {
            subscription_id,
        };
        self.send(msg).await
    }

    /// Send an acknowledgment for a received message.
    pub async fn ack(&self, msg_id: Uuid) -> Result<()> {
        let msg = Message::Ack { msg_id };
        self.send(msg).await
    }
}
```

### Phase 5: Benchmarking Harness

**Step 5.1 — Create benchmark module**

File: `crates/shellwego-network/src/quinn/bus/bench.rs`

```rust
/// Micro-benchmarks for the message bus.
///
/// Run with: cargo bench --package shellwego-network -- quic_bus
///
/// Measures:
/// - Pub/sub throughput (messages/sec) for varying message sizes
/// - Topic matching performance (wildcard vs exact)
/// - Router dispatch latency (p50, p95, p99)
/// - Memory allocation per message cycle
```

Add `criterion` to `Cargo.toml` dev-dependencies:

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "quic_bus"
harness = false
```

Benchmarks:
1. `bench_topic_exact_match` — 1M exact-match lookups against a router with 1000 subscriptions.
2. `bench_topic_wildcard_match` — 1M wildcard-match lookups (pattern `agent.*`) against 1000 subscriptions.
3. `bench_publish_single_subscriber` — Publish 100K messages to a topic with 1 subscriber, measure throughput.
4. `bench_publish_100_subscribers` — Publish 100K messages to a topic with 100 matching subscribers, measure throughput and fan-out time.
5. `bench_bus_message_serialize` — Serialize/deserialize 10K `BusMessage` envelopes via postcard, measure latency and throughput.

### Phase 6: Integration with Server Connection Lifecycle

**Step 6.1 — Connection lifecycle integration**

The bus must be notified when agent connections are established and dropped. This is handled by:

1. **On connect**: `QuinnServer::run_with_bus()` accepts connection → spawns per-connection task → waits for `Register` message → registers subscriptions.
2. **On disconnect**: Per-connection task exits → `bus.remove_node(node_id)` cleans up all subscriptions and stops message delivery.
3. **On heartbeat timeout**: If no heartbeat received within `keep_alive_interval * 3`, the server closes the connection (leverages Quinn's built-in keep-alive) → triggers disconnect cleanup.

**Step 6.2 — Update module structure**

File: `crates/shellwego-network/src/quinn/mod.rs`

```rust
pub mod common;
pub mod client;
pub mod server;
pub mod bus;  // NEW

pub use server::{QuinnServer, AgentConn};
pub use client::QuinnClient;
pub use common::{Message, QuicConfig, ResourceLimits, ChannelPriority};

// NEW re-exports
pub use bus::router::{BusRouter, BusMetrics, BusError, DeadLetterEntry};
pub use bus::topic::Topic;
pub use bus::reliability::{ReliabilityLayer, MessageDedup};
```

File: `crates/shellwego-network/src/quinn/bus/mod.rs`

```rust
pub mod topic;
pub mod envelope;
pub mod router;
pub mod reliability;
pub mod bench;
```

### Phase 7: Exports & Re-exports

**Step 7.1 — Update `shellwego-schema/src/network/mod.rs`**

Add to re-exports:

```rust
pub use quinn::{
    AgentConnection, ChannelPriority, Message, QuicConfig, ResourceLimits,
    SubscriptionId, BusMessage, BusConfig,  // NEW
};
```

**Step 7.2 — Update `shellwego-schema/src/lib.rs`**

Add to the existing re-export line:

```rust
pub use network::{..., SubscriptionId, BusMessage, BusConfig, TopicError};
```

**Step 7.3 — Update `shellwego-network/Cargo.toml`**

Add dependencies:

```toml
# For bus router
dashmap = { workspace = true }
lru = "0.12"
parking_lot = "0.12"
chrono = { workspace = true }
```

**Step 7.4 — Resolve existing warnings**

Before implementing new code, fix the 7 existing warnings in `shellwego-network`:
- Remove unused `Ipv4Addr`, `Ipv6Addr` imports.
- Use or prefix with `_` the unused `config` variables.
- Review dead code in eBPF rate limiter structs (may need `#[allow(dead_code)]` if fields are for future use).

## 6. Dependencies on Other Plans

| Plan ID | Relationship | Notes |
|---|---|---|
| **01 (Security Hardening)** | Independent | No overlap. Security plan touches auth/KMS/RBAC; this plan touches network/messaging. |
| **02 (Scheduler)** | **Depends on THIS plan** | Scheduler needs to publish `ScheduleApp` messages to agents and receive `ActionResponse` acknowledgments. Cannot function without the bus. |
| **04 (Agent)** | **Depends on THIS plan** | Agent needs to subscribe to command topics, receive scheduled work, and publish heartbeats/logs. Cannot function without the bus. |
| **05 (Billing)** | Independent | Billing reads from DB, not from the bus. May later subscribe to usage events for real-time metering. |
| **06 (Storage / Volumes)** | Independent | Volume operations are synchronous API calls. |
| **07 (Observability)** | **Weak dependency** | Observability (metrics, tracing, alerts) could consume bus events for real-time dashboards. But the bus works without it. |
| **08 (Registry / OCI)** | Independent | Image pulls happen over HTTPS, not the bus. |
| **09 (API Consolidation)** | Independent | REST API handlers are unaffected. |
| **10 (Error Handling)** | Independent | Error types are separate. Bus errors are self-contained in `BusError`. |
| **11 (Testing Infra)** | **Weak dependency** | This plan creates its own unit/integration tests. The shared testing infra (Plan 11) would provide test utilities but is not required. |

**Execution order recommendation**: This plan (03) should execute **before** Plan 02 (Scheduler) and Plan 04 (Agent). It can run in parallel with Plan 01 (Security Hardening) since they touch completely different crates.

## 7. Acceptance Criteria

### Unit Tests
- [ ] `cargo test -p shellwego-schema` passes — all new types serialize/deserialize correctly with postcard and serde_json.
- [ ] `Topic::new()` validates all edge cases: empty, too long, invalid chars, empty segments.
- [ ] `Topic::matches()` works for exact, single-wildcard (`*`), and multi-wildcard (`>`) patterns.
- [ ] `BusRouter::subscribe()` / `unsubscribe()` / `remove_node()` maintain consistent state.
- [ ] `BusRouter::publish()` fan-outs to all matching subscribers, skips non-matching.
- [ ] `BusRouter::publish()` with full subscriber inbox drops message and increments `messages_dropped`.
- [ ] `MessageDedup::check_and_insert()` rejects duplicates within TTL, accepts after TTL expiry.
- [ ] `ReliabilityLayer` retries on ack timeout, moves to dead-letter after max retries.
- [ ] All 7 existing warnings in `shellwego-network` are resolved.

### Integration Tests
- [ ] Server + Client roundtrip: Client connects → subscribes to `agent.cmd.*` → server publishes `agent.cmd.schedule` → client receives it.
- [ ] Multi-subscriber fan-out: 5 clients subscribe to `node.>` → server publishes to `node.heartbeat` → all 5 receive.
- [ ] Disconnect cleanup: Client disconnects → server's `remove_node()` purges all subscriptions → subsequent publishes to those topics deliver to 0 subscribers.
- [ ] Ack/retry cycle: Server publishes → client receives but does NOT ack → reliability layer retries → client acks on second delivery → no dead-letter.
- [ ] Dead-letter: Server publishes → client never acks → after max_retries, message appears in dead-letter queue.
- [ ] `BusRouter::metrics()` reflects correct counters after a known sequence of subscribe/publish/disconnect operations.

### Performance Benchmarks
- [ ] `cargo bench --package shellwego-network` runs all 5 benchmark cases without errors.
- [ ] Topic matching: >10M exact-match lookups/sec on a single thread.
- [ ] Topic matching: >1M wildcard-match lookups/sec on a single thread (1000 subscriptions).
- [ ] Publish throughput: >500K messages/sec for single subscriber (in-process, no QUIC I/O).
- [ ] Publish fan-out: >50K messages/sec to 100 subscribers (in-process, no QUIC I/O).
- [ ] `BusMessage` postcard serialize/deserialize: <1μs per message for typical payloads.

### Build
- [ ] `cargo build -p shellwego-network` succeeds with 0 errors, 0 warnings.
- [ ] `cargo build -p shellwego-schema` succeeds with 0 errors, 0 warnings.
- [ ] `cargo clippy -p shellwego-network -- -D warnings` passes.

## 8. Estimated Complexity

**L** (Large)

Rationale:
- Phase 1 (Types): ~200 lines in schema + ~150 lines in network (topic, envelope). Low-medium complexity — mostly type definitions with validation logic.
- Phase 2 (Router): ~350 lines. Medium complexity — concurrent data structures (DashMap), broadcast fan-out, backpressure handling.
- Phase 3 (Reliability): ~250 lines. Medium-high complexity — retry logic, exponential backoff, dedup cache, dead-letter queue.
- Phase 4 (Server/Client wiring): ~300 lines. Medium complexity — async task spawning, connection lifecycle management, protocol framing.
- Phase 5 (Benchmarks): ~150 lines. Low complexity — criterion boilerplate.
- Phase 6-7 (Module structure, exports, warning fixes): ~100 lines. Low complexity.

Total: ~1,350 lines of production code + ~400 lines of test code + ~150 lines of benchmark code.

The complexity is driven primarily by the concurrent nature of the router (multiple publishers, multiple subscribers, connection lifecycle events) and the reliability layer (retry state machines, dedup, dead-letter). However, each component is well-isolated and testable independently.

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **DashMap iteration performance** — `publish()` iterates all subscriptions on every message. With 10K+ subscriptions, this becomes O(n) per publish. | Medium | Medium — scales linearly, not logarithmically. | Phase 2 uses direct iteration for correctness. Add a secondary `DashMap<Topic, Vec<SubId>>` index in a future optimization if benchmarks show it's needed. For initial deployment (dozens of nodes, not thousands), direct iteration is fine. |
| **Backpressure starvation** — slow subscribers cause bounded mpsc channels to fill, dropping messages. | Medium | High — message loss for slow consumers. | This is by design (bounded channels prevent unbounded memory growth). The `messages_dropped` metric alerts operators. Subscribers that can't keep up should be scaled or their `subscriber_buffer_size` increased. Log at `warn` level on every drop. |
| **Topic validation too strict** — rejecting valid topic names breaks consumers. | Low | Medium — connection errors during subscribe. | Start strict (as specified), relax if needed. The `Topic::new()` constructor returns `TopicError` with clear messages. Add a `Topic::new_unchecked()` for internal use where validation has already been done. |
| **Dead-letter queue memory growth** — permanently failed messages accumulate. | Low | Medium — memory leak over long uptime. | Cap the DLQ at `max_inflight` entries (drop oldest when full). Expose a `drain_dead_letter()` method. The `subscribe_dead_letter()` broadcast channel allows monitoring. |
| **postcard deserialization mismatch** — schema changes between CP and Agent versions cause deserialization failures. | Medium | High — messages silently dropped or panics. | postcard is version-sensitive by design. Add a `version: u8` field to `BusMessage` envelope. On deserialization error, log the raw bytes for debugging and increment a `messages_malformed` counter. |
| **Dependency conflicts** — adding `lru`, `parking_lot`, `dashmap` may conflict with existing crate versions. | Low | Low — all are well-maintained, minimal transitive deps. | `dashmap` is already a workspace dependency. `parking_lot` and `lru` are lightweight. Verify with `cargo tree -p shellwego-network` after adding. |
| **Existing warning fixes break something** — resolving 7 warnings might remove code that other crates depend on. | Low | Medium | Warnings are unused imports/variables/dead fields. Dead fields in eBPF structs should be `#[allow(dead_code)]` if they represent future API. Run full `cargo test` workspace after fixes. |
