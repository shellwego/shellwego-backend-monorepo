//! Micro-benchmarks for the message bus.
//!
//! Run with: `cargo bench --package shellwego-network -- quic_bus`
//!
//! Measures:
//! - Pub/sub throughput (messages/sec) for varying subscriber counts
//! - Topic matching performance (wildcard vs exact)
//! - Router dispatch latency (p50, p95, p99)
//! - Bus message serialization/deserialization throughput

#[cfg(test)]
use crate::quinn::bus::envelope::{decode_bus_message, encode_bus_message};
#[cfg(test)]
use crate::quinn::bus::router::BusRouter;
#[cfg(test)]
use crate::quinn::bus::topic::Topic;
#[cfg(test)]
use shellwego_schema::{BusConfig, BusMessage, ChannelPriority, Message};

#[cfg(test)]
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

#[cfg(test)]
fn bench_topic_exact_match(c: &mut Criterion) {
    let topic = Topic::new("agent.heartbeat").unwrap();
    let pattern = Topic::new("agent.heartbeat").unwrap();

    c.bench_function("topic_exact_match", |b| {
        b.iter(|| black_box(pattern.matches(black_box(&topic))));
    });
}

#[cfg(test)]
fn bench_topic_wildcard_match(c: &mut Criterion) {
    let topic = Topic::new("agent.heartbeat").unwrap();
    let pattern = Topic::new("agent.*").unwrap();

    c.bench_function("topic_wildcard_match", |b| {
        b.iter(|| black_box(pattern.matches(black_box(&topic))));
    });
}

#[cfg(test)]
fn bench_topic_multi_level_wildcard(c: &mut Criterion) {
    let topic = Topic::new("node.status.cpu.memory.usage").unwrap();
    let pattern = Topic::new("node.>").unwrap();

    c.bench_function("topic_multi_level_wildcard", |b| {
        b.iter(|| black_box(pattern.matches(black_box(&topic))));
    });
}

#[cfg(test)]
fn bench_publish_single_subscriber(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let router = BusRouter::new(BusConfig::default());
    let node_id = uuid::Uuid::new_v4();

    let (_sub_id, mut rx) = rt.block_on(async {
        router.subscribe(node_id, Topic::new("bench.topic").unwrap()).unwrap()
    });

    // Drain the receiver in a background task
    rt.spawn(async move {
        while rx.recv().await.is_some() {}
    });

    let topic = Topic::new("bench.topic").unwrap();

    c.bench_function("publish_single_subscriber", |b| {
        b.iter(|| {
            let msg = BusMessage::new(
                topic.clone(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            );
            black_box(router.publish(&topic, msg));
        });
    });
}

#[cfg(test)]
fn bench_publish_no_subscribers(c: &mut Criterion) {
    let router = BusRouter::new(BusConfig::default());
    let topic = Topic::new("bench.nosub").unwrap();

    c.bench_function("publish_no_subscribers", |b| {
        b.iter(|| {
            let msg = BusMessage::new(
                topic.clone(),
                Message::Ping {
                    timestamp: chrono::Utc::now(),
                },
                ChannelPriority::Command,
            );
            black_box(router.publish(&topic, msg));
        });
    });
}

#[cfg(test)]
fn bench_bus_message_encode_decode(c: &mut Criterion) {
    let topic = Topic::new("agent.cmd.schedule").unwrap();
    let msg = BusMessage::new(
        topic,
        Message::Heartbeat {
            node_id: uuid::Uuid::new_v4(),
            cpu_usage: 0.75,
            memory_usage: 0.42,
        },
        ChannelPriority::Metrics,
    )
    .with_source(uuid::Uuid::new_v4());

    let encoded = encode_bus_message(&msg).unwrap();

    c.bench_function("bus_message_encode", |b| {
        b.iter(|| black_box(encode_bus_message(black_box(&msg)).unwrap()));
    });

    c.bench_function("bus_message_decode", |b| {
        b.iter(|| black_box(decode_bus_message(black_box(&encoded)).unwrap()));
    });
}

#[cfg(test)]
fn bench_router_subscribe_unsubscribe(c: &mut Criterion) {
    let router = BusRouter::new(BusConfig::default());

    c.bench_function("router_subscribe_unsubscribe", |b| {
        b.iter(|| {
            let node_id = uuid::Uuid::new_v4();
            let topic = Topic::new("bench.cycle").unwrap();
            let (sub_id, _rx) = router.subscribe(node_id, topic).unwrap();
            black_box(router.unsubscribe(sub_id));
        });
    });
}

#[cfg(test)]
fn bench_topic_validation(c: &mut Criterion) {
    c.bench_function("topic_validation_concrete", |b| {
        b.iter(|| black_box(Topic::new("agent.cmd.schedule")));
    });

    c.bench_function("topic_validation_wildcard", |b| {
        b.iter(|| black_box(Topic::new("node.>")));
    });

    c.bench_function("topic_validation_invalid", |b| {
        b.iter(|| black_box(Topic::new("invalid@topic!name")));
    });
}

#[cfg(test)]
criterion_group!(
    benches,
    bench_topic_exact_match,
    bench_topic_wildcard_match,
    bench_topic_multi_level_wildcard,
    bench_publish_single_subscriber,
    bench_publish_no_subscribers,
    bench_bus_message_encode_decode,
    bench_router_subscribe_unsubscribe,
    bench_topic_validation,
);

#[cfg(test)]
criterion_main!(benches);
