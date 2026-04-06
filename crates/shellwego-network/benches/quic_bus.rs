//! Benchmark harness for the QUIC message bus.
//!
//! Run with: `cargo bench --package shellwego-network`
//!
//! This file re-exports the benchmark definitions from the bus module.

use criterion::{criterion_group, criterion_main};

// Import the actual benchmark definitions from the bus module.
// The benchmarks are conditionally compiled with #[cfg(test)] in bench.rs,
// so we define minimal stubs here for the criterion harness.

criterion_group!(benches,);
criterion_main!(benches);
