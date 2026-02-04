//! App entity - re-exported from shellwego-core for Sea-ORM compatibility.
//!
//! This module provides the Sea-ORM entity for the App table.
//! The actual entity definition lives in shellwego-core to avoid duplication.

pub use shellwego_core::entities::app::{Entity as Apps, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};
