//! ZFS CLI wrapper
//!
//! Executes `zfs` and `zpool` commands with structured output parsing.

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, trace};

use crate::zfs::PoolMetrics;
use crate::SnapshotInfo;
use crate::StorageError;
use crate::VolumeInfo;

/// ZFS command interface
#[derive(Clone)]
pub struct ZfsCli {
    timeout: Duration,
}

impl ZfsCli {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Verify zfs/zpool binaries exist
    pub async fn check_prereqs(&self) -> Result<(), StorageError> {
        for bin in &["zfs", "zpool"] {
            let mut cmd = Command::new("which");
            cmd.arg(bin).kill_on_drop(true);
            match cmd.output().await {
                Ok(o) if o.status.success() => continue,
                _ => return Err(StorageError::ZfsCommand(format!("{} not found", bin))),
            }
        }
        Ok(())
    }

    /// Verify pool exists and is healthy
    pub async fn check_pool(&self, pool: &str) -> Result<(), StorageError> {
        let mut cmd = Command::new("zpool");
        cmd.args(["list", "-H", "-o", "health", pool]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("pool check timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::NotFound(format!("pool: {}", pool)));
        }

        let health = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if health != "ONLINE" {
            return Err(StorageError::ZfsCommand(format!(
                "Pool {} is {}",
                pool, health
            )));
        }

        Ok(())
    }

    pub async fn dataset_exists(&self, name: &str) -> Result<bool, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["list", "-H", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let status = cmd.status().await?;
        Ok(status.success())
    }

    pub async fn create_dataset(
        &self,
        name: &str,
        parent: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("create").kill_on_drop(true);

        if parent.is_some() {
            cmd.arg("-p");
        }

        cmd.arg(name);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("create dataset timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;
        self.check_output(output, &format!("create {}", name))
    }

    /// Create a ZFS block volume (zvol) for raw block device access
    pub async fn create_zvol(
        &self,
        name: &str,
        size_gb: u64,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "create",
            "-V", &format!("{}G", size_gb),
            "-b", "4k",
            name,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("create zvol timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;
        self.check_output(output, &format!("create zvol {}", name))
    }

    pub async fn destroy_dataset(&self, name: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("destroy").kill_on_drop(true);

        if force {
            cmd.arg("-r");
        }

        cmd.arg(name);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("destroy timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;
        self.check_output(output, &format!("destroy {}", name))
    }

    pub async fn snapshot(&self, dataset: &str, snap_name: &str) -> Result<(), StorageError> {
        let full = format!("{}@{}", dataset, snap_name);
        let mut cmd = Command::new("zfs");
        cmd.args(["snapshot", &full]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("snapshot timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("snapshot {}", full))
    }

    pub async fn create_snapshot(
        &self,
        dataset: &str,
        snap_name: &str,
    ) -> Result<(), StorageError> {
        self.snapshot(dataset, snap_name).await
    }

    pub async fn clone_snapshot(&self, snapshot: &str, target: &str) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["clone", snapshot, target]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("clone timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("clone {} to {}", snapshot, target))
    }

    pub async fn promote(&self, dataset: &str) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["promote", dataset]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("promote timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("promote {}", dataset))
    }

    pub async fn rollback(&self, snapshot: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("rollback").kill_on_drop(true);

        if force {
            cmd.arg("-r");
        }

        cmd.arg(snapshot);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("rollback timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("rollback {}", snapshot))
    }

    pub async fn set_property(
        &self,
        dataset: &str,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["set", &format!("{}={}", key, value), dataset]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("set property timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("set {}={} on {}", key, value, dataset))
    }

    pub async fn get_property(&self, dataset: &str, key: &str) -> Result<String, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["get", "-H", "-o", "value", key, dataset]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("get property timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get compression ratio for a dataset
    pub async fn get_compression_ratio(&self, dataset: &str) -> Result<f64, StorageError> {
        let raw = self.get_property(dataset, "compressratio").await?;
        // ZFS reports compressratio as "1.00x" string
        let ratio: f64 = raw.trim_end_matches('x').parse()
            .map_err(|e| StorageError::Parse(format!("Invalid compressratio '{}': {}", raw, e)))?;
        Ok(ratio)
    }

    /// Check if a dataset has encryption enabled
    pub async fn is_encrypted(&self, dataset: &str) -> Result<bool, StorageError> {
        match self.get_property(dataset, "encryption").await {
            Ok(val) => Ok(val == "on" || val == "aes-256-gcm" || val == "aes-128-gcm"),
            Err(_) => Ok(false),
        }
    }

    /// Enable ZFS native encryption on a dataset
    pub async fn enable_encryption(
        &self,
        dataset: &str,
        keyformat: &str,
        keylocation: &str,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "create",
            "-o", "encryption=on",
            "-o", &format!("keyformat={}", keyformat),
            "-o", &format!("keylocation={}", keylocation),
            "-o", "encryptionroot=off",
            dataset,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("enable encryption timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("enable encryption on {}", dataset))
    }

    /// Load encryption key for a ZFS dataset
    pub async fn load_key(
        &self,
        dataset: &str,
        keyfile_path: &str,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["load-key", "-L", &format!("file://{}", keyfile_path), dataset]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("load key timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("load key for {}", dataset))
    }

    /// Unload encryption key for a ZFS dataset
    pub async fn unload_key(&self, dataset: &str) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args(["unload-key", dataset]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("unload key timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("unload key for {}", dataset))
    }

    /// List volumes under a base dataset
    pub async fn list_volumes(
        &self,
        base: &str,
    ) -> Result<Vec<VolumeInfo>, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "list", "-H", "-p", "-r",
            "-o", "name,used,available,referenced,compressratio,mountpoint,creation",
            "-t", "filesystem,volume",
            base,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("list volumes timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::NotFound(base.to_string()));
        }

        let mut volumes = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let parts: Vec<&str> = line.trim().split('\t').collect();
            if parts.len() < 7 { continue; }
            // Skip the base dataset itself
            if parts[0] == base { continue; }

            let created_ts: i64 = parts[6].parse().unwrap_or(0);
            volumes.push(VolumeInfo {
                name: parts[0].to_string(),
                used_bytes: parts[1].parse().unwrap_or(0),
                available_bytes: parts[2].parse().unwrap_or(0),
                referenced_bytes: parts[3].parse().unwrap_or(0),
                compression_ratio: parts[4].parse().unwrap_or(1.0),
                mountpoint: if parts[5] == "-" || parts[5] == "none" {
                    None
                } else {
                    Some(parts[5].into())
                },
                created: chrono::DateTime::from_timestamp(created_ts, 0)
                    .unwrap_or_else(|| chrono::Utc::now()),
                properties: std::collections::HashMap::new(),
            });
        }
        Ok(volumes)
    }

    pub async fn mount(
        &self,
        dataset: &str,
        mountpoint: &std::path::PathBuf,
    ) -> Result<(), StorageError> {
        self.set_property(dataset, "mountpoint", &mountpoint.to_string_lossy())
            .await
    }

    pub async fn unmount(&self, dataset: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("unmount").kill_on_drop(true);

        if force {
            cmd.arg("-f");
        }

        cmd.arg(dataset);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("unmount timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        self.check_output(output, &format!("unmount {}", dataset))
    }

    pub async fn get_info(&self, dataset: &str) -> Result<VolumeInfo, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "list",
            "-H",
            "-p",
            "-o",
            "name,used,available,referenced,compressratio,mountpoint,creation",
            dataset,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("get info timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::NotFound(dataset.to_string()));
        }

        let line = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = line.trim().split('\t').collect();

        if parts.len() < 7 {
            return Err(StorageError::Parse(format!(
                "Unexpected zfs list output: {}",
                line
            )));
        }

        let created_ts: i64 = parts[6]
            .parse()
            .map_err(|e| StorageError::Parse(format!("Invalid creation timestamp: {}", e)))?;

        let properties = self.get_all_properties(dataset).await?;

        Ok(VolumeInfo {
            name: parts[0].to_string(),
            used_bytes: parts[1].parse().unwrap_or(0),
            available_bytes: parts[2].parse().unwrap_or(0),
            referenced_bytes: parts[3].parse().unwrap_or(0),
            compression_ratio: parts[4].parse().unwrap_or(1.0),
            mountpoint: if parts[5] == "-" || parts[5] == "none" {
                None
            } else {
                Some(parts[5].into())
            },
            created: chrono::DateTime::from_timestamp(created_ts, 0)
                .unwrap_or_else(|| chrono::Utc::now()),
            properties,
        })
    }

    async fn get_all_properties(
        &self,
        dataset: &str,
    ) -> Result<std::collections::HashMap<String, String>, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "get",
            "-H",
            "-p",
            "-o",
            "name,property,value",
            "all",
            dataset,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("get properties timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Ok(std::collections::HashMap::new());
        }

        let mut properties = std::collections::HashMap::new();

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let property = parts[1].to_string();
                let value = parts[2].to_string();
                if property != "name" {
                    properties.insert(property, value);
                }
            }
        }

        Ok(properties)
    }

    pub async fn list_snapshots(&self, dataset: &str) -> Result<Vec<SnapshotInfo>, StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.args([
            "list",
            "-H",
            "-p",
            "-t",
            "snapshot",
            "-o",
            "name,used,referenced,creation",
            "-r",
            dataset,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("list snapshots timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let mut snapshots = vec![];
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 {
                continue;
            }

            let name = parts[0].to_string();
            let dataset = name.split('@').next().unwrap_or("").to_string();

            snapshots.push(SnapshotInfo {
                name,
                dataset,
                used_bytes: parts[1].parse().unwrap_or(0),
                referenced_bytes: parts[2].parse().unwrap_or(0),
                created: chrono::DateTime::from_timestamp(parts[3].parse().unwrap_or(0), 0)
                    .unwrap_or_else(|| chrono::Utc::now()),
            });
        }

        Ok(snapshots)
    }

    pub async fn get_snapshot_info(&self, snapshot: &str) -> Result<SnapshotInfo, StorageError> {
        let snaps = self
            .list_snapshots(snapshot.split('@').next().unwrap_or(""))
            .await?;
        snaps
            .into_iter()
            .find(|s| s.name == snapshot)
            .ok_or_else(|| StorageError::SnapshotNotFound(snapshot.to_string()))
    }

    pub async fn get_pool_info(&self, pool: &str) -> Result<PoolMetrics, StorageError> {
        let mut cmd = Command::new("zpool");
        cmd.args([
            "list",
            "-H",
            "-p",
            "-o",
            "size,allocated,free,fragmentation,dedupratio",
            pool,
        ]).kill_on_drop(true);

        let output = tokio::time::timeout(self.timeout, cmd.output()).await
            .map_err(|_| StorageError::ZfsCommand(format!("pool info timed out after {:?}", self.timeout)))?
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            return Err(StorageError::NotFound(format!("pool: {}", pool)));
        }

        let line = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = line.trim().split('\t').collect();

        if parts.len() < 5 {
            return Err(StorageError::Parse(
                "Unexpected zpool list output".to_string(),
            ));
        }

        let parse = |s: &str| s.parse().unwrap_or(0);

        Ok(PoolMetrics {
            name: pool.to_string(),
            size_bytes: parse(parts[0]),
            allocated_bytes: parse(parts[1]),
            free_bytes: parse(parts[2]),
            fragmentation_percent: parts[3].trim_end_matches('%').parse().unwrap_or(0.0),
            dedup_ratio: parts[4].parse().unwrap_or(1.0),
        })
    }

    fn check_output(
        &self,
        output: std::process::Output,
        context: &str,
    ) -> Result<(), StorageError> {
        if output.status.success() {
            trace!("zfs {} succeeded", context);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Parse known ZFS error codes
            let error = match stderr.as_ref() {
                s if s.contains("dataset does not exist") => StorageError::NotFound(context.to_string()),
                s if s.contains("dataset already exists") => StorageError::AlreadyExists(context.to_string()),
                s if s.contains("permission denied") => StorageError::PermissionDenied(context.to_string()),
                s if s.contains("invalid property") || s.contains("invalid argument") => {
                    StorageError::InvalidName(context.to_string())
                }
                s if s.contains("no space left") || s.contains("out of space") => {
                    StorageError::InsufficientSpace { needed: 0, available: 0 }
                }
                s if s.contains("snapshot already exists") => {
                    StorageError::AlreadyExists(context.to_string())
                }
                s if s.contains("volume") && s.contains("does not exist") => {
                    StorageError::NotFound(context.to_string())
                }
                _ => StorageError::ZfsCommand(format!("{}: {}", context, stderr.trim())),
            };
            error!("zfs {} failed: {}", context, stderr.trim());
            Err(error)
        }
    }
}
