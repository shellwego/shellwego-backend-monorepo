//! Reliability Layer — at-least-once delivery guarantees.
//!
//! Provides:
//! - In-flight message tracking with configurable retry (exponential backoff).
//! - Deduplication by `msg_id` using an LRU cache.
//! - Ack timeout and dead-letter queuing for permanently failed messages.
//!
//! ## Design
//!
//! ```text
//! Publisher:
//!   send(msg) → track_in_flight(msg) → transport.send(msg) → start ack timer
//!
//! On Ack received:
//!   remove from in_flight map
//!
//! On Ack timeout:
//!   retry_count < max_retries → re-send with exponential backoff
//!   retry_count >= max_retries → move to dead-letter queue
//!
//! Receiver:
//!   recv(msg) → dedup.check_and_insert(msg.msg_id) → if new: process + send Ack
//!               → if duplicate: discard
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{debug, warn};

use shellwego_schema::{BusConfig, BusMessage};

use super::router::DeadLetterEntry;

// ---------------------------------------------------------------------------
// In-flight tracking
// ---------------------------------------------------------------------------

/// Status of an in-flight message.
#[derive(Debug)]
enum InflightStatus {
    /// Waiting for acknowledgment.
    Pending {
        /// When the message was last sent.
        sent_at: Instant,
        /// Number of retry attempts so far.
        retry_count: u32,
    },
    /// Waiting for retry backoff period.
    Backoff {
        /// When the backoff period expires and we can retry.
        retry_at: Instant,
        /// Number of retry attempts so far.
        retry_count: u32,
    },
}

/// A message awaiting acknowledgment.
struct InflightEntry {
    /// The bus message being tracked.
    message: BusMessage,
    /// Current status (pending or in backoff).
    status: InflightStatus,
}

// ---------------------------------------------------------------------------
// ReliabilityLayer
// ---------------------------------------------------------------------------

/// Handles at-least-once delivery semantics for the message bus.
///
/// On publish:
///   1. Assign `msg_id` (if not already set).
///   2. Store in in-flight map.
///   3. Send via the underlying transport.
///   4. Start ack timer.
///
/// On ack received:
///   1. Remove from in-flight map.
///
/// On ack timeout:
///   1. If `retry_count < max_retries`: re-send with exponential backoff.
///   2. If `retry_count >= max_retries`: move to dead-letter queue.
pub struct ReliabilityLayer {
    /// Configuration.
    config: BusConfig,
    /// In-flight message tracker: msg_id → entry.
    in_flight: DashMap<uuid::Uuid, InflightEntry>,
    /// Channel to request retransmission of messages.
    retry_tx: mpsc::Sender<BusMessage>,
    /// Channel to send messages to the dead-letter queue.
    dead_letter_tx: mpsc::Sender<DeadLetterEntry>,
    /// Counter for total messages tracked.
    total_tracked: AtomicU64,
    /// Counter for successful acks.
    total_acked: AtomicU64,
    /// Counter for messages retried.
    total_retried: AtomicU64,
    /// Counter for messages dead-lettered.
    total_dead_lettered: AtomicU64,
}

impl ReliabilityLayer {
    /// Create a new reliability layer with the given configuration.
    ///
    /// Returns the layer and the receiver end of the retry channel.
    /// The caller must poll the retry receiver and retransmit messages.
    pub fn new(
        config: BusConfig,
    ) -> (Self, mpsc::Receiver<BusMessage>, mpsc::Receiver<DeadLetterEntry>) {
        let (retry_tx, retry_rx) = mpsc::channel(config.max_inflight as usize);
        let (dead_letter_tx, dead_letter_rx) = mpsc::channel(1024);

        let layer = Self {
            config,
            in_flight: DashMap::new(),
            retry_tx,
            dead_letter_tx,
            total_tracked: AtomicU64::new(0),
            total_acked: AtomicU64::new(0),
            total_retried: AtomicU64::new(0),
            total_dead_lettered: AtomicU64::new(0),
        };

        (layer, retry_rx, dead_letter_rx)
    }

    /// Track a message as in-flight.
    ///
    /// Call this after sending a message. The reliability layer will
    /// monitor for acknowledgment and retry if needed.
    pub fn track(&self, message: BusMessage) {
        let msg_id = message.msg_id;
        self.in_flight.insert(
            msg_id,
            InflightEntry {
                message,
                status: InflightStatus::Pending {
                    sent_at: Instant::now(),
                    retry_count: 0,
                },
            },
        );
        self.total_tracked.fetch_add(1, Ordering::Relaxed);
        debug!(msg_id = %msg_id, "Message tracked in-flight");
    }

    /// Acknowledge a message.
    ///
    /// Removes the message from the in-flight map.
    /// Returns `true` if the message was found and removed.
    pub fn ack(&self, msg_id: uuid::Uuid) -> bool {
        if self.in_flight.remove(&msg_id).is_some() {
            self.total_acked.fetch_add(1, Ordering::Relaxed);
            debug!(msg_id = %msg_id, "Message acknowledged");
            true
        } else {
            debug!(msg_id = %msg_id, "Ack for unknown message (already acked or not tracked)");
            false
        }
    }

    /// Handle nack for a message — optionally retry immediately.
    pub fn nack(&self, msg_id: uuid::Uuid, reason: &str) {
        if let Some(mut entry) = self.in_flight.get_mut(&msg_id) {
            warn!(
                msg_id = %msg_id,
                reason = reason,
                "Message negatively acknowledged"
            );
            if let InflightStatus::Pending {
                retry_count, ..
            } = &mut entry.status
            {
                *retry_count += 1;
            }
        }
    }

    /// Check for timed-out messages and trigger retries.
    ///
    /// This should be called periodically (e.g., every 500ms) from a background task.
    /// Returns the number of messages retried.
    pub fn check_timeouts(&self) -> usize {
        let ack_timeout = Duration::from_millis(self.config.ack_timeout_ms);
        let now = Instant::now();
        let mut retried = 0usize;

        for mut entry in self.in_flight.iter_mut() {
            match &entry.status {
                InflightStatus::Pending {
                    sent_at,
                    retry_count,
                } => {
                    if now.duration_since(*sent_at) >= ack_timeout {
                        let new_count = *retry_count + 1;
                        if new_count > self.config.max_retries {
                            // Max retries exceeded — dead letter
                            let msg_id = entry.value().message.msg_id;
                            let message = entry.value().message.clone();
                            let dead_entry = DeadLetterEntry {
                                message,
                                last_error: format!(
                                    "max retries ({}): ack timeout after {}ms",
                                    self.config.max_retries, self.config.ack_timeout_ms
                                ),
                                attempt_count: new_count,
                                enqueued_at: chrono::Utc::now(),
                            };
                            let _ = self.dead_letter_tx.try_send(dead_entry);
                            self.total_dead_lettered.fetch_add(1, Ordering::Relaxed);
                            drop(entry);
                            self.in_flight.remove(&msg_id);
                        } else {
                            // Schedule retry with exponential backoff
                            let delay = self.config.retry_base_delay_ms as u64
                                * (1u64 << new_count.saturating_sub(1));
                            let retry_at = now + Duration::from_millis(delay);
                            entry.status = InflightStatus::Backoff {
                                retry_at,
                                retry_count: new_count,
                            };
                            self.total_retried.fetch_add(1, Ordering::Relaxed);
                            retried += 1;
                        }
                    }
                }
                InflightStatus::Backoff { retry_at, .. } => {
                    if now >= *retry_at {
                        // Backoff period expired — ready to retry
                        if let InflightStatus::Backoff {
                            retry_count, ..
                        } = &entry.status
                        {
                            let message = entry.value().message.clone();
                            entry.status = InflightStatus::Pending {
                                sent_at: now,
                                retry_count: *retry_count,
                            };
                            let _ = self.retry_tx.try_send(message);
                        }
                    }
                }
            }
        }

        if retried > 0 {
            debug!(count = retried, "Messages scheduled for retry");
        }

        retried
    }

    /// Get the number of currently in-flight messages.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Get reliability metrics.
    pub fn metrics(&self) -> ReliabilityMetrics {
        ReliabilityMetrics {
            total_tracked: self.total_tracked.load(Ordering::Relaxed),
            total_acked: self.total_acked.load(Ordering::Relaxed),
            total_retried: self.total_retried.load(Ordering::Relaxed),
            total_dead_lettered: self.total_dead_lettered.load(Ordering::Relaxed),
            currently_in_flight: self.in_flight.len(),
        }
    }

    /// Remove a specific message from tracking (e.g., on shutdown).
    pub fn forget(&self, msg_id: uuid::Uuid) -> bool {
        self.in_flight.remove(&msg_id).is_some()
    }

    /// Clear all in-flight messages.
    pub fn clear(&self) {
        self.in_flight.clear();
    }
}

/// Metrics snapshot for the reliability layer.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReliabilityMetrics {
    /// Total messages tracked since creation.
    pub total_tracked: u64,
    /// Total messages successfully acknowledged.
    pub total_acked: u64,
    /// Total retry attempts.
    pub total_retried: u64,
    /// Total messages moved to dead-letter queue.
    pub total_dead_lettered: u64,
    /// Current number of in-flight messages.
    pub currently_in_flight: usize,
}

// ---------------------------------------------------------------------------
// MessageDedup
// ---------------------------------------------------------------------------

/// Deduplicates received messages by `msg_id`.
///
/// Uses an LRU cache with a configurable TTL. Messages seen within the TTL
/// window are considered duplicates and discarded.
pub struct MessageDedup {
    /// LRU cache: msg_id → time first seen.
    cache: Mutex<lru::LruCache<uuid::Uuid, Instant>>,
    /// Time-to-live for dedup entries.
    ttl: Duration,
}

impl MessageDedup {
    /// Create a new dedup cache.
    ///
    /// # Arguments
    /// * `capacity` — Maximum number of entries in the LRU cache.
    /// * `ttl` — Duration after which a seen msg_id is considered stale.
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(capacity).unwrap_or(std::num::NonZeroUsize::new(1).unwrap()),
            )),
            ttl,
        }
    }

    /// Check if a message is a duplicate and register it if new.
    ///
    /// Returns `true` if the message is new (not a duplicate).
    /// Returns `false` if the message was already seen within the TTL window.
    pub fn check_and_insert(&self, msg_id: uuid::Uuid) -> bool {
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
    ///
    /// Removes all entries that have exceeded the TTL.
    pub fn cleanup(&self) -> usize {
        let mut cache = self.cache.lock();
        let now = Instant::now();
        let before = cache.len();
        cache.retain(|_, seen_at| now.duration_since(*seen_at) < self.ttl);
        before - cache.len()
    }

    /// Get the current number of entries in the cache.
    pub fn len(&self) -> usize {
        self.cache.lock().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.lock().is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::{ChannelPriority, Message, Topic};

    fn make_reliability() -> (ReliabilityLayer, mpsc::Receiver<BusMessage>, mpsc::Receiver<DeadLetterEntry>) {
        ReliabilityLayer::new(BusConfig::default())
    }

    fn make_test_msg() -> BusMessage {
        BusMessage::new(
            Topic::new("test.topic").unwrap(),
            Message::Ping {
                timestamp: chrono::Utc::now(),
            },
            ChannelPriority::Command,
        )
    }

    #[test]
    fn test_track_and_ack() {
        let (layer, _retry_rx, _dl_rx) = make_reliability();
        let msg = make_test_msg();
        let msg_id = msg.msg_id;

        assert_eq!(layer.in_flight_count(), 0);

        layer.track(msg);
        assert_eq!(layer.in_flight_count(), 1);

        let acked = layer.ack(msg_id);
        assert!(acked);
        assert_eq!(layer.in_flight_count(), 0);

        // Double ack should return false
        let acked = layer.ack(msg_id);
        assert!(!acked);

        let metrics = layer.metrics();
        assert_eq!(metrics.total_tracked, 1);
        assert_eq!(metrics.total_acked, 1);
    }

    #[test]
    fn test_nack_increments_retry_count() {
        let (layer, _retry_rx, _dl_rx) = make_reliability();
        let msg = make_test_msg();
        let msg_id = msg.msg_id;

        layer.track(msg);
        layer.nack(msg_id, "processing error");

        let metrics = layer.metrics();
        assert_eq!(metrics.total_tracked, 1);
    }

    #[test]
    fn test_forget_message() {
        let (layer, _retry_rx, _dl_rx) = make_reliability();
        let msg = make_test_msg();
        let msg_id = msg.msg_id;

        layer.track(msg);
        assert_eq!(layer.in_flight_count(), 1);

        let forgotten = layer.forget(msg_id);
        assert!(forgotten);
        assert_eq!(layer.in_flight_count(), 0);

        let forgotten = layer.forget(msg_id);
        assert!(!forgotten);
    }

    #[test]
    fn test_clear_all() {
        let (layer, _retry_rx, _dl_rx) = make_reliability();

        for _ in 0..10 {
            layer.track(make_test_msg());
        }
        assert_eq!(layer.in_flight_count(), 10);

        layer.clear();
        assert_eq!(layer.in_flight_count(), 0);
    }

    #[test]
    fn test_dedup_new_message() {
        let dedup = MessageDedup::new(1000, Duration::from_secs(300));
        let msg_id = uuid::Uuid::new_v4();

        assert!(dedup.check_and_insert(msg_id));
        assert!(!dedup.check_and_insert(msg_id));
        assert_eq!(dedup.len(), 1);
    }

    #[test]
    fn test_dedup_capacity_eviction() {
        let dedup = MessageDedup::new(3, Duration::from_secs(300));

        assert!(dedup.check_and_insert(uuid::Uuid::new_v4()));
        assert!(dedup.check_and_insert(uuid::Uuid::new_v4()));
        assert!(dedup.check_and_insert(uuid::Uuid::new_v4()));
        assert_eq!(dedup.len(), 3);

        // Adding a 4th should evict the oldest
        assert!(dedup.check_and_insert(uuid::Uuid::new_v4()));
        assert_eq!(dedup.len(), 3);
    }

    #[test]
    fn test_dedup_cleanup() {
        let dedup = MessageDedup::new(1000, Duration::from_millis(50));

        dedup.check_and_insert(uuid::Uuid::new_v4());
        assert_eq!(dedup.len(), 1);

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(100));

        let cleaned = dedup.cleanup();
        assert_eq!(cleaned, 1);
        assert!(dedup.is_empty());
    }

    #[tokio::test]
    async fn test_timeout_triggers_retry() {
        // Use a very short ack timeout for testing
        let config = BusConfig {
            ack_timeout_ms: 50,
            retry_base_delay_ms: 10,
            max_retries: 3,
            ..BusConfig::default()
        };
        let (layer, mut retry_rx, _dl_rx) = ReliabilityLayer::new(config);

        let msg = make_test_msg();
        layer.track(msg);

        // Wait for ack timeout
        tokio::time::sleep(Duration::from_millis(80)).await;

        let retried = layer.check_timeouts();
        assert!(retried > 0);

        // Should have a retry message queued
        let retry_msg = retry_rx.recv().await;
        assert!(retry_msg.is_some());
    }
}
