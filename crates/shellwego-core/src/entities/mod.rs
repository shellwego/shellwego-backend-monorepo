//! Domain entities for the ShellWeGo platform.
//! 
//! These structs define the wire format for API requests/responses
//! and the internal state machine representations.

pub mod app;
pub mod database;
pub mod domain;
pub mod node;
pub mod secret;
pub mod volume;

// TODO: Re-export main entity types for ergonomic imports
// pub use app::{App, AppStatus};
// pub use node::{Node, NodeStatus};
// etc.