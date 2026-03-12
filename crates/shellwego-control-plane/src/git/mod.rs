//! Git integration module
//!
//! Build queue, build executor, repository management, and webhook handling.

pub mod builder;
pub mod webhook;

pub use builder::{BuildQueue, BuildExecutor, BuildSpec, BuildStatus};
pub use webhook::{WebhookRouter, WebhookEvent};
