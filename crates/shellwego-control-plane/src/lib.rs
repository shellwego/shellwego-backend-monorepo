//! ShellWeGo Control Plane — library exports
//!
//! This crate exposes the control-plane's building blocks for integration
//! tests and for potential reuse as a library.
//!
//! # Note
//!
//! The binary entry-point lives in `main.rs` which re-exports the same
//! modules via `pub(crate)` visibility.  This `lib.rs` simply makes the
//! public API available to downstream crates (e.g. integration tests).

pub mod api;
pub mod config;
pub mod federation;
pub mod git;
pub mod kms;
pub mod orm;
pub mod services;
pub mod state;
pub mod operators;
