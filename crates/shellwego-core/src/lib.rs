//! ShellWeGo Core
//! 
//! The shared kernel. All domain entities and common types live here.
//! No business logic, just pure data structures and validation.

pub mod entities;
pub mod error;
pub mod prelude;

// Re-export commonly used types at crate root
pub use entities::*;
pub use error::{CoreError, CoreResult};