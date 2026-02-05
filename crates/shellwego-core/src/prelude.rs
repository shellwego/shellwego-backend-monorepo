//! Common imports for ShellWeGo crates.
//! 
//! Usage: `use shellwego_core::prelude::*;`

pub use chrono::{DateTime, Utc};
pub use serde::{Deserialize, Serialize};
pub use strum::{Display, EnumString};
pub use uuid::Uuid;
pub use validator::Validate;

pub use crate::error::{CoreError, CoreResult};