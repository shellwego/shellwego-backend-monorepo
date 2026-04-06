//! LUKS2 disk encryption support (optional)
//!
//! Requires `cryptsetup` on the host and the `luks2` feature flag.
//! ZFS native encryption is preferred; LUKS2 is for zvols that
//! need cross-platform compatibility or specific cipher modes.

use crate::StorageError;
use tokio::process::Command;
use tracing::{debug, info};

/// LUKS2 disk encryption manager
pub struct Luks2Manager {
    cryptsetup_path: String,
}

impl Luks2Manager {
    pub fn new() -> Result<Self, StorageError> {
        Ok(Self {
            cryptsetup_path: "cryptsetup".to_string(),
        })
    }

    /// Create a new LUKS2 manager with custom cryptsetup path
    pub fn with_path(path: &str) -> Self {
        Self {
            cryptsetup_path: path.to_string(),
        }
    }

    /// Format a block device as LUKS2
    pub async fn luks_format(
        &self,
        device: &str,
        keyfile: &str,
        cipher: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut cmd = Command::new(&self.cryptsetup_path);
        cmd.args(["luksFormat", "--type", "luks2", "--batch-mode"])
            .kill_on_drop(true);

        if let Some(c) = cipher {
            cmd.args(["--cipher", c]);
        }

        cmd.args(["--key-file", keyfile, device]);

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksFormat: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        info!("Formatted {} as LUKS2", device);
        Ok(())
    }

    /// Open a LUKS2 device
    pub async fn luks_open(
        &self,
        device: &str,
        mapper_name: &str,
        keyfile: &str,
    ) -> Result<(), StorageError> {
        let output = Command::new(&self.cryptsetup_path)
            .args(["open", "--type", "luks2", "--key-file", keyfile, device, mapper_name])
            .kill_on_drop(true)
            .output().await?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksOpen: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        debug!("Opened LUKS2 device {} as {}", device, mapper_name);
        Ok(())
    }

    /// Close a LUKS2 device
    pub async fn luks_close(&self, mapper_name: &str) -> Result<(), StorageError> {
        let output = Command::new(&self.cryptsetup_path)
            .args(["close", mapper_name])
            .kill_on_drop(true)
            .output().await?;

        if !output.status.success() {
            return Err(StorageError::ZfsCommand(format!(
                "luksClose: {}", String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }
}
