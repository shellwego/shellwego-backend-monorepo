//! Sea-ORM entity definitions
//!
//! This module re-exports entity models from shellwego-core where possible,
//! avoiding duplication between core domain types and ORM entities.

pub use shellwego_core::entities::app::{Entity as Apps, Model as AppModel, AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec, Relation as AppRelation, ActiveModel as AppActiveModel};
pub use shellwego_core::entities::app::{Entity as AppInstances, Model as AppInstanceModel, InstanceStatus, Relation as AppInstanceRelation, ActiveModel as AppInstanceActiveModel};

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
