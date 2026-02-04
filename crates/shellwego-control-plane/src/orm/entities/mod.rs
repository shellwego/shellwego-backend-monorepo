//! Sea-ORM entity definitions
//!
//! This module contains all database entity models using Sea-ORM's
//! derive macros. Each entity corresponds to a table in the database.

pub mod app;
pub mod node;
pub mod database;
pub mod domain;
pub mod secret;
pub mod volume;
pub mod organization;
pub mod user;
pub mod deployment;
pub mod backup;
pub mod webhook;
pub mod audit_log;

// TODO: Re-export all entity models
// pub use app::*;
// pub use node::*;
// etc.

// TODO: Add prelude module for common imports
// pub mod prelude {
//     pub use sea_orm::prelude::*;
//     pub use super::*;
// }
