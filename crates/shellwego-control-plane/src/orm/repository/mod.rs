//! Repository module for database access layer
//!
//! This module provides Data Access Objects (DAOs) for each entity,
//! encapsulating database operations and business logic.

pub mod app_repository;
pub mod node_repository;
pub mod database_repository;
pub mod domain_repository;
pub mod secret_repository;
pub mod volume_repository;
pub mod organization_repository;
pub mod user_repository;
pub mod deployment_repository;
pub mod backup_repository;
pub mod webhook_repository;
pub mod audit_log_repository;

// TODO: Re-export all repository traits for convenience
// TODO: Add common repository trait with shared methods
// TODO: Add transaction support for multi-repository operations
// TODO: Add caching layer for frequently accessed data
// TODO: Add query builder for complex queries
