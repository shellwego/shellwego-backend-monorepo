//! ZFS CLI wrapper
//! 
//! Executes `zfs` and `zpool` commands with structured output parsing.

use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, trace, error};

use crate::{StorageError, VolumeInfo, SnapshotInfo}; use crate::zfs::PoolMetrics;

/// ZFS command interface
#[derive(Clone)]
pub struct ZfsCli;

impl ZfsCli {
    pub fn new() -> Self {
        Self {}
    }

    /// Verify zfs/zpool binaries exist
    pub async fn check_prereqs(&self) -> Result<(), StorageError> {
        for bin in &["zfs", "zpool"] {
            match Command::new("which").arg(bin).output().await {
                Ok(o) if o.status.success() => continue,
                _ => return Err(StorageError::ZfsCommand(format!("{} not found", bin))),
            }
        }
        Ok(())
    }

    /// Verify pool exists and is healthy
    pub async fn check_pool(&self, pool: &str) -> Result<(), StorageError> {
        let output = Command::new("zpool")
            .args(["list", "-H", "-o", "health", pool])
            .output()
            .await?;
            
        if !output.status.success() {
            return Err(StorageError::NotFound(format!("pool: {}", pool)));
        }
        
        let health = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if health != "ONLINE" {
            return Err(StorageError::ZfsCommand(format!(
                "Pool {} is {}", pool, health
            )));
        }
        
        Ok(())
    }

    pub async fn dataset_exists(&self, name: &str) -> Result<bool, StorageError> {
        let status = Command::new("zfs")
            .args(["list", "-H", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
            
        Ok(status.success())
    }

    pub async fn create_dataset(
        &self,
        name: &str,
        parent: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("create");
        
        if let Some(p) = parent {
            cmd.arg("-p"); // Create parents
        }
        
        cmd.arg(name);
        
        let output = cmd.output().await?;
        self.check_output(output, &format!("create {}", name))
    }

    pub async fn destroy_dataset(&self, name: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("destroy");
        
        if force {
            cmd.arg("-r"); // Recursive
        }
        
        cmd.arg(name);
        
        let output = cmd.output().await?;
        self.check_output(output, &format!("destroy {}", name))
    }

    pub async fn snapshot(&self, dataset: &str, snap_name: &str) -> Result<(), StorageError> {
        let full = format!("{}@{}", dataset, snap_name);
        let output = Command::new("zfs")
            .args(["snapshot", &full])
            .output()
            .await?;
            
        self.check_output(output, &format!("snapshot {}", full))
    }

    pub async fn create_snapshot(
        &self,
        dataset: &str,
        snap_name: &str,
    ) -> Result<(), StorageError> {
        self.snapshot(dataset, snap_name).await
    }

    pub async fn clone_snapshot(
        &self,
        snapshot: &str,
        target: &str,
    ) -> Result<(), StorageError> {
        let output = Command::new("zfs")
            .args(["clone", snapshot, target])
            .output()
            .await?;
            
        self.check_output(output, &format!("clone {} to {}", snapshot, target))
    }

    pub async fn promote(&self, dataset: &str) -> Result<(), StorageError> {
        let output = Command::new("zfs")
            .args(["promote", dataset])
            .output()
            .await?;
            
        self.check_output(output, &format!("promote {}", dataset))
    }

    pub async fn rollback(&self, snapshot: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("rollback");
        
        if force {
            cmd.arg("-r"); // Destroy intermediate snapshots
        }
        
        cmd.arg(snapshot);
        
        let output = cmd.output().await?;
        self.check_output(output, &format!("rollback {}", snapshot))
    }

    pub async fn set_property(
        &self,
        dataset: &str,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        let output = Command::new("zfs")
            .args(["set", &format!("{}={}", key, value), dataset])
            .output()
            .await?;
            
        self.check_output(output, &format!("set {}={} on {}", key, value, dataset))
    }

    pub async fn get_property(&self, dataset: &str, key: &str) -> Result<String, StorageError> {
        let output = Command::new("zfs")
            .args(["get", "-H", "-o", "value", key, dataset])
            .output()
            .await?;
            
        if !output.status.success() {
            return Err(StorageError::ZfsCommand(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub async fn mount(&self, dataset: &str, mountpoint: &std::path::PathBuf) -> Result<(), StorageError> {
        // Set mountpoint property
        self.set_property(dataset, "mountpoint", &mountpoint.to_string_lossy()).await
    }

    pub async fn unmount(&self, dataset: &str, force: bool) -> Result<(), StorageError> {
        let mut cmd = Command::new("zfs");
        cmd.arg("unmount");
        
        if force {
            cmd.arg("-f");
        }
        
        cmd.arg(dataset);
        
        let output = cmd.output().await?;
        self.check_output(output, &format!("unmount {}", dataset))
    }

    pub async fn get_info(&self, dataset: &str) -> Result<VolumeInfo, StorageError> {
        let output = Command::new("zfs")
            .args([
                "list",
                "-H",
                "-p",
                "-o",
                "name,used,available,referenced,compressratio,mountpoint,creation",
                dataset,
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(StorageError::NotFound(dataset.to_string()));
        }

        let line = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = line.trim().split('\t').collect();

        if parts.len() < 7 {
            return Err(StorageError::Parse(format!("Unexpected zfs list output: {}", line)));
        }

        let created_ts: i64 = parts[6].parse().map_err(|e| {
            StorageError::Parse(format!("Invalid creation timestamp: {}", e))
        })?;

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

    async fn get_all_properties(&self, dataset: &str) -> Result<std::collections::HashMap<String, String>, StorageError> {
        let output = Command::new("zfs")
            .args([
                "get",
                "-H",
                "-p",
                "-o", "name,property,value",
                "all",
                dataset,
            ])
            .output()
            .await?;

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

    pub async fn list_snapshots(
        &self,
        dataset: &str,
    ) -> Result<Vec<SnapshotInfo>, StorageError> {
        let output = Command::new("zfs")
            .args([
                "list",
                "-H",
                "-p",
                "-t", "snapshot",
                "-o", "name,used,referenced,creation",
                "-r", dataset,
            ])
            .output()
            .await?;
            
        if !output.status.success() {
            return Err(StorageError::ZfsCommand(
                String::from_utf8_lossy(&output.stderr).to_string()
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
        let snaps = self.list_snapshots(snapshot.split('@').next().unwrap_or("")).await?;
        snaps.into_iter()
            .find(|s| s.name == snapshot)
            .ok_or_else(|| StorageError::SnapshotNotFound(snapshot.to_string()))
    }

    pub async fn get_pool_info(&self, pool: &str) -> Result<PoolMetrics, StorageError> {
        let output = Command::new("zpool")
            .args([
                "list",
                "-H",
                "-p",
                "-o",
                "size,allocated,free,fragmentation,dedupratio",
                pool,
            ])
            .output()
            .await?;
            
        if !output.status.success() {
            return Err(StorageError::NotFound(format!("pool: {}", pool)));
        }
        
        let line = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = line.trim().split('\t').collect();
        
        if parts.len() < 5 {
            return Err(StorageError::Parse("Unexpected zpool list output".to_string()));
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

    fn check_output(&self, output: std::process::Output, context: &str) -> Result<(), StorageError> {
        if output.status.success() {
            trace!("zfs {} succeeded", context);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("zfs {} failed: {}", context, stderr);
            Err(StorageError::ZfsCommand(format!("{}: {}", context, stderr)))
        }
    }
}