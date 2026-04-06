//! Encryption at rest for volumes using AES-256-GCM
//!
//! Architecture:
//!   - DEK (Data Encryption Key): Random 256-bit key, used to encrypt data
//!   - KEK (Key Encryption Key): Master key from KMS/hardware
//!   - DEK is encrypted (wrapped) by KEK for storage
//!   - Only encrypted DEK is stored; plaintext DEK exists briefly in memory
//!
//! Encryption flow:
//!   1. Generate random DEK (32 bytes)
//!   2. Encrypt DEK with KEK -> wrapped_dek
//!   3. Store wrapped_dek alongside encrypted data
//!   4. Decrypt: fetch wrapped_dek, decrypt with KEK -> plaintext DEK

use crate::StorageError;
use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac as _};
use rand::RngCore;
use sha2::Sha256;
use std::fmt;
use crate::zfs::ZfsManager;
use std::sync::Arc;
use thiserror::Error;

const DEK_SIZE: usize = 32;
const IV_SIZE: usize = 12;
const TAG_SIZE: usize = 16;
const HMAC_SIZE: usize = 32;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Key generation failed: {0}")]
    KeyGen(String),
    #[error("Encryption failed: {0}")]
    Encrypt(String),
    #[error("Decryption failed: {0}")]
    Decrypt(String),
    #[error("Invalid key format")]
    InvalidKey,
    #[error("Authentication failed - data may be tampered")]
    AuthFailed,
}

impl From<EncryptionError> for StorageError {
    fn from(e: EncryptionError) -> Self {
        StorageError::Backend(format!("Encryption: {}", e))
    }
}

pub struct EncryptionProvider {
    master_key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    pub master_key: String,
    #[allow(dead_code)]
    pub algorithm: Option<String>,
}

impl EncryptionProvider {
    pub async fn new(config: &EncryptionConfig) -> Result<Self, EncryptionError> {
        let master_key = hex::decode(&config.master_key)
            .map_err(|e| EncryptionError::KeyGen(format!("Invalid hex: {}", e)))?;

        if master_key.len() != 32 {
            return Err(EncryptionError::KeyGen(format!(
                "Master key must be 32 bytes, got {}",
                master_key.len()
            )));
        }

        Ok(EncryptionProvider { master_key })
    }

    pub async fn generate_dek(&self) -> Result<DataKey, EncryptionError> {
        let mut plaintext_dek = vec![0u8; DEK_SIZE];
        OsRng.fill_bytes(&mut plaintext_dek);

        let (wrapped_dek, iv) = self.wrap_dek(&plaintext_dek)?;

        Ok(DataKey {
            ciphertext: wrapped_dek,
            iv,
            master_key_id: "local".to_string(),
        })
    }

    pub async fn decrypt_dek(
        &self,
        encrypted_dek: &[u8],
        iv: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        self.unwrap_dek(encrypted_dek, iv)
    }

    pub fn encrypt_block(
        &self,
        plaintext: &[u8],
        key: &[u8],
        iv: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| EncryptionError::Encrypt(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(iv);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::Encrypt(format!("AEAD error: {}", e)))?;

        Ok(ciphertext)
    }

    pub fn decrypt_block(
        &self,
        ciphertext: &[u8],
        key: &[u8],
        iv: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| EncryptionError::Decrypt(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(iv);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::AuthFailed)?;

        Ok(plaintext)
    }

    fn wrap_dek(&self, plaintext_dek: &[u8]) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
        let iv = self.generate_iv();
        let encrypted = self.encrypt_block(plaintext_dek, &self.master_key, &iv)?;

        let mut result = encrypted;
        let tag = self.compute_hmac(&result);
        result.extend_from_slice(&tag);

        Ok((result, iv))
    }

    fn unwrap_dek(&self, encrypted_dek: &[u8], iv: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if encrypted_dek.len() < HMAC_SIZE + DEK_SIZE {
            return Err(EncryptionError::InvalidKey);
        }

        let (ciphertext_with_gcm_tag, expected_hmac_bytes) = encrypted_dek.split_at(encrypted_dek.len() - HMAC_SIZE);
        let ciphertext_with_gcm_tag = ciphertext_with_gcm_tag.to_vec();

        let actual_hmac = self.compute_hmac(&ciphertext_with_gcm_tag);
        let expected_hmac: [u8; HMAC_SIZE] = expected_hmac_bytes
            .try_into()
            .map_err(|_| EncryptionError::InvalidKey)?;

        if actual_hmac.as_slice() != expected_hmac.as_slice() {
            return Err(EncryptionError::AuthFailed);
        }

        self.decrypt_block(&ciphertext_with_gcm_tag, &self.master_key, iv)
    }

    pub fn generate_iv(&self) -> Vec<u8> {
        let mut iv = vec![0u8; IV_SIZE];
        OsRng.fill_bytes(&mut iv);
        iv
    }

    fn compute_hmac(&self, data: &[u8]) -> Vec<u8> {
        let mut mac: Hmac<Sha256> =
            hmac::Mac::new_from_slice(&self.master_key).expect("HMAC key size valid");
        mac.update(data);
        let result = mac.finalize().into_bytes();
        result.to_vec()
    }
}

/// Manages volume-level encryption operations
pub struct VolumeEncryptor {
    provider: Arc<EncryptionProvider>,
    zfs: Arc<ZfsManager>,
    keys_dir: std::path::PathBuf,
}

impl VolumeEncryptor {
    pub fn new(
        provider: Arc<EncryptionProvider>,
        zfs: Arc<ZfsManager>,
        keys_dir: std::path::PathBuf,
    ) -> Self {
        Self { provider, zfs, keys_dir }
    }

    /// Encrypt a volume using ZFS native encryption
    pub async fn encrypt_volume(
        &self,
        volume_id: uuid::Uuid,
    ) -> Result<EncryptionStatus, StorageError> {
        // 1. Generate raw ZFS encryption key
        let mut raw_key = vec![0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw_key);

        // 2. Wrap with master key
        let dek = self.provider.generate_dek().await
            .map_err(|e| StorageError::Encryption(format!("DEK generation: {}", e)))?;

        // 3. Store wrapped key
        tokio::fs::create_dir_all(&self.keys_dir).await?;
        let wrapped_path = self.keys_dir.join(format!("{}.wrapped", volume_id));
        let encoded = dek.to_base64();
        tokio::fs::write(&wrapped_path, &encoded).await?;

        // 4. Write raw key to temp file
        let temp_key_path = self.keys_dir.join(format!("{}.raw.tmp", volume_id));
        tokio::fs::write(&temp_key_path, &raw_key).await?;

        // 5. Load key into ZFS
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        let result = self.zfs.cli().load_key(&dataset, temp_key_path.to_str().unwrap()).await;

        // 6. Secure-delete temp key
        tokio::fs::write(&temp_key_path, vec![0u8; 32]).await.ok();
        tokio::fs::remove_file(&temp_key_path).await.ok();

        result.map(|()| EncryptionStatus::ZfsNative)
    }

    /// Unlock an encrypted volume
    pub async fn unlock_volume(&self, volume_id: uuid::Uuid) -> Result<(), StorageError> {
        let wrapped_path = self.keys_dir.join(format!("{}.wrapped", volume_id));
        let encoded = tokio::fs::read_to_string(&wrapped_path).await
            .map_err(|_| StorageError::NotFound(format!("wrapped key for {}", volume_id)))?;

        let dek = DataKey::from_base64(&encoded)
            .map_err(|e| StorageError::Encryption(format!("DEK parse: {}", e)))?;

        let raw_key = self.provider.decrypt_dek(&dek.ciphertext, &dek.iv).await
            .map_err(|e| StorageError::Encryption(format!("Key unwrap: {}", e)))?;

        let temp_key_path = self.keys_dir.join(format!("{}.raw.tmp", volume_id));
        tokio::fs::write(&temp_key_path, &raw_key).await?;

        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        let result = self.zfs.cli().load_key(&dataset, temp_key_path.to_str().unwrap()).await;

        tokio::fs::write(&temp_key_path, vec![0u8; 32]).await.ok();
        tokio::fs::remove_file(&temp_key_path).await.ok();

        result
    }

    /// Lock (unload key) for a volume
    pub async fn lock_volume(&self, volume_id: uuid::Uuid) -> Result<(), StorageError> {
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        self.zfs.cli().unload_key(&dataset).await
    }

    /// Check encryption status of a volume
    pub async fn get_encryption_status(&self, volume_id: uuid::Uuid) -> Result<EncryptionStatus, StorageError> {
        let dataset = self.zfs.full_path(&format!("volumes/{}", volume_id));
        if self.zfs.cli().is_encrypted(&dataset).await? {
            Ok(EncryptionStatus::ZfsNative)
        } else {
            Ok(EncryptionStatus::Unencrypted)
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataKey {
    pub ciphertext: Vec<u8>,
    pub iv: Vec<u8>,
    pub master_key_id: String,
}

impl DataKey {
    pub fn encrypted_bytes(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn iv(&self) -> &[u8] {
        &self.iv
    }

    pub fn master_key_id(&self) -> &str {
        &self.master_key_id
    }

    pub fn to_base64(&self) -> String {
        let master_key_id_bytes = self.master_key_id.as_bytes();
        let mki_len_bytes = (master_key_id_bytes.len() as u32).to_be_bytes();
        let iv_len_bytes = (self.iv.len() as u32).to_be_bytes();
        let ciphertext_len_bytes = (self.ciphertext.len() as u32).to_be_bytes();

        let mut combined = Vec::with_capacity(12 + master_key_id_bytes.len() + self.iv.len() + self.ciphertext.len());
        combined.extend_from_slice(&mki_len_bytes);
        combined.extend_from_slice(master_key_id_bytes);
        combined.extend_from_slice(&iv_len_bytes);
        combined.extend_from_slice(&self.iv);
        combined.extend_from_slice(&ciphertext_len_bytes);
        combined.extend_from_slice(&self.ciphertext);

        STANDARD.encode(combined)
    }

    pub fn from_base64(s: &str) -> Result<Self, EncryptionError> {
        let combined = STANDARD
            .decode(s)
            .map_err(|_| EncryptionError::InvalidKey)?;

        if combined.len() < 4 {
            return Err(EncryptionError::InvalidKey);
        }

        let mki_len = u32::from_be_bytes(combined[0..4].try_into().unwrap()) as usize;
        if combined.len() < 4 + mki_len + 8 {
            return Err(EncryptionError::InvalidKey);
        }

        let master_key_id = String::from_utf8(combined[4..4 + mki_len].to_vec())
            .map_err(|_| EncryptionError::InvalidKey)?;
        let rest = &combined[4 + mki_len..];

        let iv_len = u32::from_be_bytes(rest[0..4].try_into().unwrap()) as usize;
        if rest.len() < 8 + iv_len {
            return Err(EncryptionError::InvalidKey);
        }

        let ciphertext_len =
            u32::from_be_bytes(rest[4 + iv_len..8 + iv_len].try_into().unwrap()) as usize;

        if rest.len() != 8 + iv_len + ciphertext_len {
            return Err(EncryptionError::InvalidKey);
        }

        let iv = rest[4..4 + iv_len].to_vec();
        let ciphertext = rest[8 + iv_len..].to_vec();

        Ok(DataKey {
            ciphertext,
            iv,
            master_key_id,
        })
    }
}

/// Encryption status for a volume
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EncryptionStatus {
    /// No encryption applied
    Unencrypted,
    /// ZFS native encryption (encryption=on, keyformat=raw/passphrase)
    ZfsNative,
    /// LUKS2 container (cryptsetup)
    Luks2,
    /// Application-level envelope encryption (DEK/KEK)
    ApplicationLevel,
}

impl std::fmt::Display for EncryptionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionStatus::Unencrypted => write!(f, "unencrypted"),
            EncryptionStatus::ZfsNative => write!(f, "zfs_native"),
            EncryptionStatus::Luks2 => write!(f, "luks2"),
            EncryptionStatus::ApplicationLevel => write!(f, "application_level"),
        }
    }
}

impl fmt::Display for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DataKey(iv={}, ciphertext_len={})",
            self.iv.len(),
            self.ciphertext.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let config = EncryptionConfig {
            master_key: hex::encode(vec![0u8; 32]),
            algorithm: None,
        };

        let provider = EncryptionProvider::new(&config).await.unwrap();
        let dek = provider.generate_dek().await.unwrap();
        let plaintext_dek = provider
            .decrypt_dek(&dek.ciphertext, &dek.iv)
            .await
            .unwrap();

        assert_eq!(plaintext_dek.len(), 32);

        let test_data = b"Hello, World! This is a test of encryption.";
        let iv = provider.generate_iv();
        let encrypted = provider
            .encrypt_block(test_data, &plaintext_dek, &iv)
            .unwrap();
        let decrypted = provider
            .decrypt_block(&encrypted, &plaintext_dek, &iv)
            .unwrap();

        assert_eq!(&decrypted, test_data);
    }

    #[test]
    fn test_datakey_base64_roundtrip() {
        let original = DataKey {
            ciphertext: vec![1u8, 2, 3, 4],
            iv: vec![5u8, 6, 7, 8, 9, 10, 11, 12],
            master_key_id: "test".to_string(),
        };

        let encoded = original.to_base64();
        let restored = DataKey::from_base64(&encoded).unwrap();

        assert_eq!(restored.ciphertext, original.ciphertext);
        assert_eq!(restored.iv, original.iv);
        assert_eq!(restored.master_key_id, original.master_key_id);
    }

    #[tokio::test]
    async fn test_tampered_data_detection() {
        let config = EncryptionConfig {
            master_key: hex::encode(vec![0u8; 32]),
            algorithm: None,
        };

        let provider = EncryptionProvider::new(&config).await.unwrap();
        let dek = provider.generate_dek().await.unwrap();

        let mut tampered = dek.ciphertext.clone();
        tampered[0] ^= 0xff;

        let result = provider.decrypt_dek(&tampered, &dek.iv).await;
        assert!(result.is_err());
    }
}
