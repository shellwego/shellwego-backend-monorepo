//! Storage management for ShellWeGo
//! 
//! Abstracts ZFS operations for container rootfs and persistent volumes.
//! All dataset operations go through this crate for consistency and safety.

use std::path::PathBuf;
use thiserror::Error;

pub mod zfs;

pub use zfs::ZfsManager;

/// Storage backend trait for pluggability
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Create a new dataset/volume
    async fn create(&self, name: &str, size: u64) -> Result<VolumeInfo, StorageError>;
    
    /// Destroy dataset and all snapshots
    async fn destroy(&self, name: &str, force: bool) -> Result<(), StorageError>;
    
    /// Create snapshot
    async fn snapshot(&self, source: &str, snap_name: &str) -> Result<SnapshotInfo, StorageError>;
    
    /// Clone from snapshot
    async fn clone(&self, snap: &str, target: &str) -> Result<VolumeInfo, StorageError>;
    
    /// Rollback to snapshot
    async fn rollback(&self, snap: &str, force: bool) -> Result<(), StorageError>;
    
    /// List datasets
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<VolumeInfo>, StorageError>;
    
    /// Get dataset info
    async fn info(&self, name: &str) -> Result<VolumeInfo, StorageError>;
    
    /// Mount dataset to host path
    async fn mount(&self, name: &str, mountpoint: &PathBuf) -> Result<(), StorageError>;
    
    /// Unmount
    async fn unmount(&self, name: &str) -> Result<(), StorageError>;
    
    /// Set property (quota, compression, etc)
    async fn set_property(&self, name: &str, key: &str, value: &str) -> Result<(), StorageError>;
    
    /// Get property
    async fn get_property(&self, name: &str, key: &str) -> Result<String, StorageError>;
}

/// Volume/dataset information
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub name: String,
    pub mountpoint: Option<PathBuf>,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub referenced_bytes: u64,
    pub compression_ratio: f64,
    pub created: chrono::DateTime<chrono::Utc>,
    pub properties: std::collections::HashMap<String, String>,
}

/// Snapshot metadata
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub name: String,
    pub dataset: String,
    pub created: chrono::DateTime<chrono::Utc>,
    pub used_bytes: u64,
    pub referenced_bytes: u64,
}

/// Storage operation errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("ZFS command failed: {0}")]
    ZfsCommand(String),
    
    #[error("Dataset not found: {0}")]
    NotFound(String),
    
    #[error("Dataset already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),
    
    #[error("Insufficient space: needed {needed}MB, available {available}MB")]
    InsufficientSpace { needed: u64, available: u64 },
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Invalid name: {0}")]
    InvalidName(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(String),
}

/// Helper to sanitize dataset names
pub fn sanitize_name(name: &str) -> Result<String, StorageError> {
    // ZFS names can contain: letters, numbers, underscore, hyphen, colon, period, slash
    // We restrict further for safety
    let sanitized: String = name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
        
    if sanitized.is_empty() || sanitized.len() > 255 {
        return Err(StorageError::InvalidName(name.to_string()));
    }
    
    Ok(sanitized)
}