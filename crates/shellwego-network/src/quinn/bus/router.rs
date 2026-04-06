//! Message Bus Router — the core pub/sub routing engine.
//!
//! `BusRouter` maintains the subscription table and dispatches published messages
//! to all matching subscribers via bounded MPSC channels. Backpressure is implicit
//! — when a subscriber's inbox is full, messages are dropped and counted.
//!
//! ## Usage
//!
//! ```ignore
//! let router = BusRouter::new(BusConfig::default());
//! let topic = Topic::new("agent.cmd.schedule")?;
//! let (sub_id, rx) = router.subscribe(node_id, Topic::new("agent.*")?)?;
//!
//! let msg = BusMessage::new(topic, payload, ChannelPriority::Command);
//! let delivered = router.publish(&topic, msg);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

use shellwego_schema::{BusConfig, BusMessage, SubscriptionId, Topic};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A subscriber entry in the routing table.
struct SubscriberEntry {
    /// The topic pattern this subscriber is listening on.
    topic_pattern: Topic,
    /// Channel to deliver messages to this subscriber.
    inbox: mpsc::Sender<BusMessage>,
    /// Node ID of the subscriber (for routing awareness).
    node_id: uuid::Uuid,
}

/// Dead-letter entry for messages that failed all retry attempts.
#[derive(Debug, Clone)]
pub struct DeadLetterEntry {
    /// The message that failed.
    pub message: BusMessage,
    /// Last error that caused the dead-letter.
    pub last_error: String,
    /// Number of delivery attempts before dead-lettering.
    pub attempt_count: u32,
    /// When the message was moved to the dead-letter queue.
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

/// Error type for bus operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BusError {
    #[error("too many subscribers for topic '{topic}' (limit: {limit})")]
    TooManySubscribers { topic: String, limit: usize },
    #[error("subscription not found: {0}")]
    NotFound(SubscriptionId),
    #[error("topic error: {0}")]
    Topic(#[from] shellwego_schema::TopicError),
}

/// Metrics snapshot from the bus router.
#[derive(Debug, Clone, Copy, Default)]
pub struct BusMetrics {
    /// Total number of messages published.
    pub messages_published: u64,
    /// Total number of messages delivered to subscribers.
    pub messages_delivered: u64,
    /// Total number of messages dropped (inbox full or closed).
    pub messages_dropped: u64,
    /// Total number of messages moved to the dead-letter queue.
    pub messages_dead_lettered: u64,
    /// Current number of active subscriptions.
    pub active_subscriptions: usize,
    /// Current number of connected nodes (nodes with at least one subscription).
    pub connected_nodes: usize,
}

// ---------------------------------------------------------------------------
// BusRouter
// ---------------------------------------------------------------------------

/// The core message bus router.
///
/// This is the brain of the pub/sub system. It:
/// 1. Maintains a `DashMap` of `SubscriptionId → SubscriberEntry`.
/// 2. On `publish()`, iterates all subscribers and does topic matching.
/// 3. Delivers via bounded `mpsc::Sender` — backpressure is implicit (try_send fails when full).
/// 4. Optionally stores dead-lettered messages.
pub struct BusRouter {
    /// Subscription registry: subscription_id → subscriber info.
    subscriptions: DashMap<SubscriptionId, SubscriberEntry>,
    /// Per-node subscriptions: node_id → Vec<subscription_id> (for cleanup on disconnect).
    node_subscriptions: DashMap<uuid::Uuid, Vec<SubscriptionId>>,
    /// Configuration.
    config: BusConfig,
    /// Dead-letter queue (in-memory buffer).
    dead_letter: parking_lot::Mutex<Vec<DeadLetterEntry>>,
    /// Dead-letter broadcast channel (for monitoring).
    dead_letter_tx: broadcast::Sender<DeadLetterEntry>,
    /// Monotonic counter for subscription IDs.
    subscription_counter: AtomicU64,
    /// Metrics counters.
    messages_published: AtomicU64,
    messages_delivered: AtomicU64,
    messages_dropped: AtomicU64,
    messages_dead_lettered: AtomicU64,
}

impl BusRouter {
    /// Create a new bus router with the given configuration.
    pub fn new(config: BusConfig) -> Self {
        let (dead_letter_tx, _) = broadcast::channel(1024);
        Self {
            subscriptions: DashMap::new(),
            node_subscriptions: DashMap::new(),
            config,
            dead_letter: parking_lot::Mutex::new(Vec::new()),
            dead_letter_tx,
            subscription_counter: AtomicU64::new(1),
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
        node_id: uuid::Uuid,
        topic_pattern: Topic,
    ) -> Result<(SubscriptionId, mpsc::Receiver<BusMessage>), BusError> {
        // Check subscriber limit per topic
        let count = self
            .subscriptions
            .iter()
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
    ///
    /// Returns `true` if the subscription existed and was removed.
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
            info!(
                sub_id = sub_id.0,
                node_id = %entry.node_id,
                "Subscriber removed"
            );
            true
        } else {
            false
        }
    }

    /// Remove all subscriptions for a node (called on disconnect).
    ///
    /// Cleans up all routing entries for the given node and returns
    /// the number of subscriptions removed.
    pub fn remove_node(&self, node_id: uuid::Uuid) -> usize {
        if let Some((_, sub_ids)) = self.node_subscriptions.remove(&node_id) {
            let count = sub_ids.len();
            for sub_id in &sub_ids {
                self.subscriptions.remove(sub_id);
            }
            info!(
                node_id = %node_id,
                count = count,
                "All subscriptions removed for node"
            );
            count
        } else {
            0
        }
    }

    /// Publish a message to a topic.
    ///
    /// This iterates all subscribers and delivers to every matching one.
    /// Returns the number of subscribers the message was delivered to.
    ///
    /// Messages are delivered via `try_send` — if a subscriber's inbox is full,
    /// the message is dropped and counted as a backpressure drop. If the
    /// subscriber's channel is closed (receiver dropped), it's flagged for
    /// cleanup in the next sweep.
    pub fn publish(&self, topic: &Topic, message: BusMessage) -> usize {
        self.messages_published.fetch_add(1, Ordering::Relaxed);

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
                        // Subscriber receiver dropped — stale entry.
                        // Cannot modify DashMap during iteration;
                        // stale cleanup happens in sweep_stale().
                        warn!(
                            sub_id = entry.key().0,
                            "Subscriber channel closed, will be swept"
                        );
                    }
                }
            }
        }

        self.messages_delivered
            .fetch_add(delivered as u64, Ordering::Relaxed);
        self.messages_dropped
            .fetch_add(dropped as u64, Ordering::Relaxed);

        if delivered == 0 && dropped == 0 {
            debug!(topic = topic.as_str(), "No subscribers for topic");
        }

        delivered
    }

    /// Background task to sweep stale subscriptions (where receiver is dropped).
    ///
    /// Returns the number of stale subscriptions removed.
    pub async fn sweep_stale(&self) -> usize {
        let stale: Vec<SubscriptionId> = self
            .subscriptions
            .iter()
            .filter(|entry| entry.inbox.is_closed())
            .map(|entry| *entry.key())
            .collect();

        for sub_id in &stale {
            self.unsubscribe(*sub_id);
        }

        stale.len()
    }

    /// Add a message to the dead-letter queue.
    pub fn add_dead_letter(&self, entry: DeadLetterEntry) {
        if !self.config.dead_letter_enabled {
            return;
        }
        let mut dl = self.dead_letter.lock();
        // Keep the dead-letter queue bounded at 10,000 entries
        if dl.len() >= 10_000 {
            dl.remove(0);
        }
        let _ = self.dead_letter_tx.send(entry.clone());
        dl.push(entry);
        self.messages_dead_lettered.fetch_add(1, Ordering::Relaxed);
    }

    /// Get all dead-letter entries.
    pub fn dead_letter_entries(&self) -> Vec<DeadLetterEntry> {
        self.dead_letter.lock().clone()
    }

    /// Subscribe to dead-letter events (for monitoring).
    pub fn subscribe_dead_letter(&self) -> broadcast::Receiver<DeadLetterEntry> {
        self.dead_letter_tx.subscribe()
    }

    /// Clear the dead-letter queue.
    pub fn clear_dead_letter(&self) {
        self.dead_letter.lock().clear();
    }

    /// Get a snapshot of bus metrics.
    pub fn metrics(&self) -> BusMetrics {
        BusMetrics {
            messages_published: self.messages_published.load(Ordering::Relaxed),
            messages_delivered: self.messages_delivered.load(Ordering::Relaxed),
            messages_dropped: self.messages_dropped.load(Ordering::Relaxed),
            messages_dead_lettered: self.messages_dead_lettered.load(Ordering::Relaxed),
            active_subscriptions: self.subscriptions.len(),
            connected_nodes: self.node_subscriptions.len(),
        }
    }

    /// Get the number of active subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Get the number of connected nodes.
    pub fn node_count(&self) -> usize {
        self.node_subscriptions.len()
    }

    fn next_subscription_id(&self) -> u64 {
        self.subscription_counter.fetch_add(1, Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::{BusConfig, ChannelPriority, Message, Topic};

    fn make_router() -> BusRouter {
        BusRouter::new(BusConfig::default())
    }

    #[test]
    fn test_subscribe_and_publish() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();
        let pattern = Topic::new("agent.heartbeat").unwrap();
        let (sub_id, mut rx) = router.subscribe(node_id, pattern).unwrap();

        let topic = Topic::new("agent.heartbeat").unwrap();
        let msg = BusMessage::new(
            topic,
            Message::Heartbeat {
                node_id,
                cpu_usage: 0.5,
                memory_usage: 0.3,
            },
            ChannelPriority::Metrics,
        );

        let delivered = router.publish(&Topic::new("agent.heartbeat").unwrap(), msg);
        assert_eq!(delivered, 1);

        let received = rx.blocking_recv().unwrap();
        assert_eq!(received.topic.as_str(), "agent.heartbeat");
        assert_eq!(received.priority, ChannelPriority::Metrics);

        // Clean up
        router.unsubscribe(sub_id);
    }

    #[test]
    fn test_wildcard_matching() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();
        let pattern = Topic::new("agent.*").unwrap();
        let (sub_id, mut rx) = router.subscribe(node_id, pattern).unwrap();

        // Should match single-segment wildcard
        let delivered = router.publish(
            &Topic::new("agent.heartbeat").unwrap(),
            BusMessage::new(
                Topic::new("agent.heartbeat").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        assert_eq!(delivered, 1);

        // Should NOT match multi-segment topic
        let delivered = router.publish(
            &Topic::new("agent.cmd.schedule").unwrap(),
            BusMessage::new(
                Topic::new("agent.cmd.schedule").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        assert_eq!(delivered, 0);

        // Should NOT match a completely different topic
        let delivered = router.publish(
            &Topic::new("system.ping").unwrap(),
            BusMessage::new(
                Topic::new("system.ping").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        assert_eq!(delivered, 0);

        // Verify we got exactly one message
        assert!(rx.blocking_recv().is_some());
        assert!(rx.blocking_recv().is_none());

        router.unsubscribe(sub_id);
    }

    #[test]
    fn test_multi_level_wildcard() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();
        let pattern = Topic::new("node.>").unwrap();
        let (sub_id, mut rx) = router.subscribe(node_id, pattern).unwrap();

        // Should match all sub-topics
        router.publish(
            &Topic::new("node.heartbeat").unwrap(),
            BusMessage::new(
                Topic::new("node.heartbeat").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        router.publish(
            &Topic::new("node.status.cpu").unwrap(),
            BusMessage::new(
                Topic::new("node.status.cpu").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        router.publish(
            &Topic::new("node.a.b.c.d").unwrap(),
            BusMessage::new(
                Topic::new("node.a.b.c.d").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );

        // Should NOT match a different top-level topic
        let delivered = router.publish(
            &Topic::new("agent.heartbeat").unwrap(),
            BusMessage::new(
                Topic::new("agent.heartbeat").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        assert_eq!(delivered, 0);

        // Should have received 3 messages
        assert!(rx.blocking_recv().is_some());
        assert!(rx.blocking_recv().is_some());
        assert!(rx.blocking_recv().is_some());
        assert!(rx.blocking_recv().is_none());

        router.unsubscribe(sub_id);
    }

    #[test]
    fn test_remove_node() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();

        router
            .subscribe(node_id, Topic::new("agent.*").unwrap())
            .unwrap();
        router
            .subscribe(node_id, Topic::new("system.>").unwrap())
            .unwrap();

        assert_eq!(router.subscription_count(), 2);
        assert_eq!(router.node_count(), 1);

        let removed = router.remove_node(node_id);
        assert_eq!(removed, 2);
        assert_eq!(router.subscription_count(), 0);
        assert_eq!(router.node_count(), 0);
    }

    #[test]
    fn test_metrics() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();
        let (sub_id, mut rx) = router
            .subscribe(node_id, Topic::new("test.topic").unwrap())
            .unwrap();

        router.publish(
            &Topic::new("test.topic").unwrap(),
            BusMessage::new(
                Topic::new("test.topic").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        router.publish(
            &Topic::new("test.topic").unwrap(),
            BusMessage::new(
                Topic::new("test.topic").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );

        let metrics = router.metrics();
        assert_eq!(metrics.messages_published, 2);
        assert_eq!(metrics.messages_delivered, 2);
        assert_eq!(metrics.active_subscriptions, 1);

        // Drop the receiver to simulate disconnect
        drop(rx);
        router.unsubscribe(sub_id);

        let metrics = router.metrics();
        assert_eq!(metrics.active_subscriptions, 0);
    }

    #[test]
    fn test_dead_letter_queue() {
        let router = make_router();

        let entry = DeadLetterEntry {
            message: BusMessage::new(
                Topic::new("test").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
            last_error: "max retries exceeded".to_string(),
            attempt_count: 3,
            enqueued_at: chrono::Utc::now(),
        };

        router.add_dead_letter(entry.clone());
        router.add_dead_letter(DeadLetterEntry {
            message: BusMessage::new(
                Topic::new("test2").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
            last_error: "connection lost".to_string(),
            attempt_count: 1,
            enqueued_at: chrono::Utc::now(),
        });

        let entries = router.dead_letter_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].last_error, "max retries exceeded");

        let metrics = router.metrics();
        assert_eq!(metrics.messages_dead_lettered, 2);

        router.clear_dead_letter();
        assert_eq!(router.dead_letter_entries().len(), 0);
    }

    #[tokio::test]
    async fn test_sweep_stale() {
        let router = make_router();
        let node_id = uuid::Uuid::new_v4();
        let (_sub_id, rx) = router
            .subscribe(node_id, Topic::new("test").unwrap())
            .unwrap();

        assert_eq!(router.subscription_count(), 1);

        // Drop the receiver to make it stale
        drop(rx);

        let swept = router.sweep_stale().await;
        assert_eq!(swept, 1);
        assert_eq!(router.subscription_count(), 0);
    }

    #[test]
    fn test_multiple_subscribers_same_topic() {
        let router = make_router();
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let (sub_id1, mut rx1) = router
            .subscribe(node1, Topic::new("agent.heartbeat").unwrap())
            .unwrap();
        let (sub_id2, mut rx2) = router
            .subscribe(node2, Topic::new("agent.heartbeat").unwrap())
            .unwrap();

        let delivered = router.publish(
            &Topic::new("agent.heartbeat").unwrap(),
            BusMessage::new(
                Topic::new("agent.heartbeat").unwrap(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            ),
        );
        assert_eq!(delivered, 2);

        // Both should receive the message
        assert!(rx1.blocking_recv().is_some());
        assert!(rx2.blocking_recv().is_some());

        router.unsubscribe(sub_id1);
        router.unsubscribe(sub_id2);
    }
}
