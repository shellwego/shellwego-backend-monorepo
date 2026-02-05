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

use std::fmt;
use aes_gcm::KeyInit;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, OsRng};
use sha2::Sha256;
use hmac::{Hmac, Mac as _};
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use thiserror::Error;
use crate::StorageError;

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
    pub algorithm: Option<String>,
}

impl EncryptionProvider {
    pub async fn new(config: &EncryptionConfig) -> Result<Self, EncryptionError> {
        let master_key = hex::decode(&config.master_key)
            .map_err(|e| EncryptionError::KeyGen(format!("Invalid hex: {}", e)))?;

        if master_key.len() != 32 {
            return Err(EncryptionError::KeyGen(
                format!("Master key must be 32 bytes, got {}", master_key.len())
            ));
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

    pub async fn decrypt_dek(&self, encrypted_dek: &[u8], iv: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        self.unwrap_dek(encrypted_dek, iv)
    }

    pub fn encrypt_block(&self, plaintext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| EncryptionError::Encrypt(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(iv);

        let ciphertext = cipher.encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::Encrypt(format!("AEAD error: {}", e)))?;

        Ok(ciphertext)
    }

    pub fn decrypt_block(&self, ciphertext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| EncryptionError::Decrypt(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(iv);

        let plaintext = cipher.decrypt(nonce, ciphertext)
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
        if encrypted_dek.len() < TAG_SIZE + IV_SIZE {
            return Err(EncryptionError::InvalidKey);
        }

        let (ciphertext, expected_tag) = encrypted_dek.split_at(encrypted_dek.len() - HMAC_SIZE);
        let ciphertext = ciphertext.to_vec();

        let actual_tag = self.compute_hmac(&ciphertext);
        let expected_hmac: [u8; HMAC_SIZE] = expected_tag.try_into()
            .map_err(|_| EncryptionError::InvalidKey)?;

        use subtle::ConstantTimeEq;
        if actual_tag.as_slice() != expected_hmac.as_slice() {
            return Err(EncryptionError::AuthFailed);
        }

        self.decrypt_block(&ciphertext, &self.master_key, iv)
    }

    fn generate_iv(&self) -> Vec<u8> {
        let mut iv = vec![0u8; IV_SIZE];
        OsRng.fill_bytes(&mut iv);
        iv
    }

    fn compute_hmac(&self, data: &[u8]) -> Vec<u8> {
        let mut mac = <Hmac<Sha256> as hmac::Mac>::new_from_slice(&self.master_key)
            .expect("HMAC key size valid");
        mac.update(data);
        let result = mac.finalize().into_bytes();
        result.to_vec()
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
        let iv_len = self.iv.len() as u32;
        let ct_len = self.ciphertext.len() as u32;
        let id_bytes = self.master_key_id.as_bytes();
        let id_len = id_bytes.len() as u32;
        let combined = [
            &iv_len.to_be_bytes()[..],
            &self.iv[..],
            &ct_len.to_be_bytes()[..],
            &self.ciphertext[..],
            &id_len.to_be_bytes()[..],
            id_bytes,
        ]
        .concat();
        base64::engine::general_purpose::STANDARD.encode(combined)
    }

    pub fn from_base64(s: &str) -> Result<Self, EncryptionError> {
        use base64::Engine;
        let combined = base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| EncryptionError::InvalidKey)?;
        if combined.len() < 12 { return Err(EncryptionError::InvalidKey); }

        let mut offset = 0;
        let iv_len = u32::from_be_bytes(combined[offset..offset+4].try_into().unwrap()) as usize;
        offset += 4;
        if combined.len() < offset + iv_len + 4 { return Err(EncryptionError::InvalidKey); }
        let iv = combined[offset..offset+iv_len].to_vec();
        offset += iv_len;

        let ct_len = u32::from_be_bytes(combined[offset..offset+4].try_into().unwrap()) as usize;
        offset += 4;
        if combined.len() < offset + ct_len + 4 { return Err(EncryptionError::InvalidKey); }
        let ciphertext = combined[offset..offset+ct_len].to_vec();
        offset += ct_len;

        let id_len = u32::from_be_bytes(combined[offset..offset+4].try_into().unwrap()) as usize;
        offset += 4;
        if combined.len() < offset + id_len { return Err(EncryptionError::InvalidKey); }
        let master_key_id = String::from_utf8(combined[offset..offset+id_len].to_vec()).map_err(|_| EncryptionError::InvalidKey)?;

        Ok(DataKey { ciphertext, iv, master_key_id })
    }
    }

impl fmt::Display for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DataKey(iv={}, ciphertext_len={})", self.iv.len(), self.ciphertext.len())
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
        let plaintext_dek = provider.decrypt_dek(&dek.ciphertext, &dek.iv).await.unwrap();

        assert_eq!(plaintext_dek.len(), 32);

        let test_data = b"Hello, World! This is a test of encryption.";
        let iv = provider.generate_iv();
        let encrypted = provider.encrypt_block(test_data, &plaintext_dek, &iv).unwrap();
        let decrypted = provider.decrypt_block(&encrypted, &plaintext_dek, &iv).unwrap();

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
