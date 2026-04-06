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

// Suppress warnings for scaffolding/scaffolded code that is not yet wired into
// actual functionality.  These modules contain structs, methods, enums and
// fields that are defined for future use but not currently referenced.
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

pub mod api;
pub mod audit;
pub mod auth;
pub mod config;
pub mod federation;
pub mod git;
pub mod kms;
pub mod orm;
pub mod services;
pub mod state;
pub mod operators;
