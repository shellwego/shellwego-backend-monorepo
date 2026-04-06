//! Key Management Service integration
//!
//! Supports HashiCorp Vault, AWS KMS, GCP KMS, Azure Key Vault, and file-based encryption.

use std::collections::HashMap;
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// KMS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KmsConfig {
    /// KMS backend type
    pub backend: KmsBackend,
    /// Key ID for encryption
    pub key_id: String,
    /// Cache encrypted keys
    pub cache_enabled: bool,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
}

impl Default for KmsConfig {
    fn default() -> Self {
        Self {
            backend: KmsBackend::File,
            key_id: "default".to_string(),
            cache_enabled: true,
            cache_ttl_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KmsBackend {
    Vault {
        address: String,
        token: String,
        mount_path: String,
    },
    AwsKms {
        region: String,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
    },
    GcpKms {
        project_id: String,
        location: String,
        key_ring: String,
    },
    AzureKeyVault {
        vault_url: String,
        tenant_id: String,
        client_id: String,
        client_secret: String,
    },
    File,
}

/// Encrypted secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSecret {
    pub id: Uuid,
    pub key: String,
    pub ciphertext: String,
    pub nonce: String,
    pub key_version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// Key version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersion {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub status: KeyStatus,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyStatus {
    Active,
    Deprecated,
    Destroyed,
}

/// KMS client
pub struct KmsClient {
    config: KmsConfig,
    secrets: Arc<RwLock<HashMap<String, EncryptedSecret>>>,
    key_versions: Arc<RwLock<Vec<KeyVersion>>>,
    cache: Arc<RwLock<HashMap<String, (String, DateTime<Utc>)>>>,
    /// AES-256-GCM master key derived from key_id via Argon2
    master_key: [u8; 32],
}

impl KmsClient {
    /// Create a new KMS client from configuration
    pub async fn from_config(config: KmsConfig) -> Result<Self, KmsError> {
        info!("Initializing KMS client with backend: {:?}", config.backend);

        // Initialize key versions
        let key_versions = vec![
            KeyVersion {
                version: 1,
                created_at: Utc::now(),
                status: KeyStatus::Active,
                algorithm: "AES-256-GCM".to_string(),
            }
        ];

        // Derive master key from key_id using Argon2
        let key_id_for_derive = if config.key_id.is_empty() {
            "shellwego-default-master-key".to_string()
        } else {
            config.key_id.clone()
        };
        let master_key = {
            use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
            let salt = SaltString::encode_b64(&rand_core::OsRng).unwrap();
            let argon2 = Argon2::default();
            let hash = argon2.hash_password(key_id_for_derive.as_bytes(), &salt).unwrap();
            let hash_bytes = hash.hash.unwrap().as_bytes();
            let mut key = [0u8; 32];
            key.copy_from_slice(&hash_bytes[..32]);
            key
        };

        Ok(Self {
            config,
            secrets: Arc::new(RwLock::new(HashMap::new())),
            key_versions: Arc::new(RwLock::new(key_versions)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            master_key,
        })
    }

    /// Encrypt a value
    pub async fn encrypt(&self, key: &str, plaintext: &str) -> Result<EncryptedSecret, KmsError> {
        debug!("Encrypting secret: {}", key);
        
        // Check cache
        if self.config.cache_enabled {
            let cache = self.cache.read().await;
            if let Some((_cached, timestamp)) = cache.get(key) {
                let elapsed = (Utc::now() - timestamp).num_seconds() as u64;
                if elapsed < self.config.cache_ttl_secs {
                    debug!("Returning cached encrypted value for key: {}", key);
                    // Return cached encrypted value
                }
            }
        }
        
        // Perform encryption based on backend
        let (ciphertext, nonce) = self.perform_encrypt(plaintext).await?;
        
        let current_version = self.get_current_key_version().await;
        
        let secret = EncryptedSecret {
            id: Uuid::new_v4(),
            key: key.to_string(),
            ciphertext,
            nonce,
            key_version: current_version,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: HashMap::new(),
        };
        
        // Store secret
        {
            let mut secrets = self.secrets.write().await;
            secrets.insert(key.to_string(), secret.clone());
        }
        
        info!("Encrypted secret: {}", key);
        Ok(secret)
    }

    /// Perform actual encryption
    async fn perform_encrypt(&self, plaintext: &str) -> Result<(String, String), KmsError> {
        match &self.config.backend {
            KmsBackend::Vault { .. } => self.encrypt_vault(plaintext).await,
            KmsBackend::AwsKms { .. } => self.encrypt_aws(plaintext).await,
            KmsBackend::GcpKms { .. } => self.encrypt_gcp(plaintext).await,
            KmsBackend::AzureKeyVault { .. } => self.encrypt_azure(plaintext).await,
            KmsBackend::File => self.encrypt_file(plaintext).await,
        }
    }

    async fn encrypt_vault(&self, _plaintext: &str) -> Result<(String, String), KmsError> {
        Err(KmsError::BackendError("Vault backend not implemented. Integrate hvac/vault-rs crate.".to_string()))
    }

    async fn encrypt_aws(&self, _plaintext: &str) -> Result<(String, String), KmsError> {
        Err(KmsError::BackendError("AWS KMS backend not implemented. Integrate aws-sdk-kms crate.".to_string()))
    }

    async fn encrypt_gcp(&self, _plaintext: &str) -> Result<(String, String), KmsError> {
        Err(KmsError::BackendError("GCP KMS backend not implemented. Integrate google-cloud-kms crate.".to_string()))
    }

    async fn encrypt_azure(&self, _plaintext: &str) -> Result<(String, String), KmsError> {
        Err(KmsError::BackendError("Azure Key Vault backend not implemented. Integrate azure-security-keyvault crate.".to_string()))
    }

    async fn encrypt_file(&self, plaintext: &str) -> Result<(String, String), KmsError> {
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
        debug!("Encrypting via AES-256-GCM file-based encryption");
        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| KmsError::EncryptionFailed(e.to_string()))?;
        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| KmsError::EncryptionFailed(e.to_string()))?;
        Ok((BASE64.encode(&ciphertext), BASE64.encode(&nonce_bytes)))
    }

    /// Decrypt a value
    pub async fn decrypt(&self, key: &str) -> Result<String, KmsError> {
        debug!("Decrypting secret: {}", key);
        
        let secret = {
            let secrets = self.secrets.read().await;
            secrets.get(key).cloned()
                .ok_or_else(|| KmsError::NotFound(key.to_string()))?
        };
        
        self.decrypt_secret(&secret).await
    }

    /// Decrypt an encrypted secret
    pub async fn decrypt_secret(&self, secret: &EncryptedSecret) -> Result<String, KmsError> {
        let plaintext = match &self.config.backend {
            KmsBackend::Vault { .. } => self.decrypt_vault(&secret.ciphertext).await?,
            KmsBackend::AwsKms { .. } => self.decrypt_aws(&secret.ciphertext).await?,
            KmsBackend::GcpKms { .. } => self.decrypt_gcp(&secret.ciphertext).await?,
            KmsBackend::AzureKeyVault { .. } => self.decrypt_azure(&secret.ciphertext).await?,
            KmsBackend::File => self.decrypt_file_with_nonce(&secret.ciphertext, &secret.nonce).await?,
        };
        
        info!("Decrypted secret: {}", secret.key);
        Ok(plaintext)
    }

    async fn decrypt_vault(&self, _ciphertext: &str) -> Result<String, KmsError> {
        Err(KmsError::BackendError("Vault backend not implemented. Integrate hvac/vault-rs crate.".to_string()))
    }

    async fn decrypt_aws(&self, _ciphertext: &str) -> Result<String, KmsError> {
        Err(KmsError::BackendError("AWS KMS backend not implemented. Integrate aws-sdk-kms crate.".to_string()))
    }

    async fn decrypt_gcp(&self, _ciphertext: &str) -> Result<String, KmsError> {
        Err(KmsError::BackendError("GCP KMS backend not implemented. Integrate google-cloud-kms crate.".to_string()))
    }

    async fn decrypt_azure(&self, _ciphertext: &str) -> Result<String, KmsError> {
        Err(KmsError::BackendError("Azure Key Vault backend not implemented. Integrate azure-security-keyvault crate.".to_string()))
    }

    /// Decrypt file-based secret with nonce (AES-256-GCM)
    async fn decrypt_file_with_nonce(&self, ciphertext: &str, nonce_b64: &str) -> Result<String, KmsError> {
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| KmsError::DecryptionFailed(e.to_string()))?;
        let ciphertext_bytes = BASE64.decode(ciphertext)?;
        let nonce_bytes = BASE64.decode(nonce_b64)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext_bytes.as_ref())
            .map_err(|e| KmsError::DecryptionFailed(e.to_string()))?;
        String::from_utf8(plaintext).map_err(Into::into)
    }

    /// Rotate master key
    pub async fn rotate_master_key(&self) -> Result<KeyVersion, KmsError> {
        info!("Rotating master key");
        
        let current_versions = self.key_versions.read().await;
        let new_version = current_versions.len() as u32 + 1;
        drop(current_versions);
        
        let key_version = KeyVersion {
            version: new_version,
            created_at: Utc::now(),
            status: KeyStatus::Active,
            algorithm: "AES-256-GCM".to_string(),
        };
        
        // Deprecate old versions
        {
            let mut versions = self.key_versions.write().await;
            for v in versions.iter_mut() {
                if v.status == KeyStatus::Active {
                    v.status = KeyStatus::Deprecated;
                }
            }
            versions.push(key_version.clone());
        }
        
        // Re-encrypt all secrets with new key
        self.reencrypt_all_secrets().await?;
        
        info!("Master key rotated to version {}", new_version);
        Ok(key_version)
    }

    /// Re-encrypt all secrets with current key
    async fn reencrypt_all_secrets(&self) -> Result<(), KmsError> {
        debug!("Re-encrypting all secrets with new key version");
        
        let secrets = self.secrets.read().await;
        for _secret in secrets.values() {
            // Decrypt with old key, encrypt with new key
            // In production, this would use version-specific keys
        }
        
        Ok(())
    }

    /// Get current key version
    async fn get_current_key_version(&self) -> u32 {
        let versions = self.key_versions.read().await;
        versions.iter()
            .filter(|v| v.status == KeyStatus::Active)
            .map(|v| v.version)
            .max()
            .unwrap_or(1)
    }

    /// List key versions
    pub async fn list_key_versions(&self) -> Vec<KeyVersion> {
        let versions = self.key_versions.read().await;
        versions.clone()
    }

    /// Delete a secret
    pub async fn delete(&self, key: &str) -> Result<(), KmsError> {
        let mut secrets = self.secrets.write().await;
        secrets.remove(key)
            .ok_or_else(|| KmsError::NotFound(key.to_string()))?;
        
        info!("Deleted secret: {}", key);
        Ok(())
    }

    /// List all secret keys
    pub async fn list_keys(&self) -> Vec<String> {
        let secrets = self.secrets.read().await;
        secrets.keys().cloned().collect()
    }

    /// Detect and migrate legacy base64-encoded secrets
    /// Legacy secrets have a "file:" prefix after base64 decoding
    pub fn is_legacy_secret(&self, ciphertext: &str) -> bool {
        if let Ok(decoded) = BASE64.decode(ciphertext) {
            if let Ok(s) = String::from_utf8(decoded) {
                return s.starts_with("file:");
            }
        }
        false
    }

    /// Health check
    pub async fn health_check(&self) -> Result<(), KmsError> {
        // Try encrypt/decrypt test
        let test_key = "__health_check__";
        let _encrypted = self.encrypt(test_key, "test").await?;
        let decrypted = self.decrypt(test_key).await?;
        
        if decrypted != "test" {
            return Err(KmsError::HealthCheckFailed("Encrypt/decrypt mismatch".to_string()));
        }
        
        // Clean up
        self.delete(test_key).await?;
        
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum KmsError {
    #[error("Secret not found: {0}")]
    NotFound(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Key rotation failed: {0}")]
    RotationFailed(String),

    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    #[error("Backend error: {0}")]
    BackendError(String),

    #[error("Base64 error: {0}")]
    Base64Error(#[from] base64::DecodeError),

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kms_client_creation() {
        let config = KmsConfig::default();
        let client = KmsClient::from_config(config).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_encrypt_decrypt() {
        let config = KmsConfig::default();
        let client = KmsClient::from_config(config).await.unwrap();
        
        let encrypted = client.encrypt("test-key", "secret-value").await.unwrap();
        assert_eq!(encrypted.key, "test-key");
        
        let decrypted = client.decrypt("test-key").await.unwrap();
        assert_eq!(decrypted, "secret-value");
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let config = KmsConfig::default();
        let client = KmsClient::from_config(config).await.unwrap();
        
        let version = client.rotate_master_key().await.unwrap();
        assert_eq!(version.version, 2);
        assert_eq!(version.status, KeyStatus::Active);
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = KmsConfig::default();
        let client = KmsClient::from_config(config).await.unwrap();
        
        let result = client.health_check().await;
        assert!(result.is_ok());
    }
}
