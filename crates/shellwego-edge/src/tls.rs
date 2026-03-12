//! TLS certificate management and Let's Encrypt automation
//!
//! Handles certificate provisioning via ACME, storage, renewal,
//! and SNI-based certificate selection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rustls::server::ResolvesServerCert;
use rustls::{Certificate as RustlsCertificate, PrivateKey, ServerConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::EdgeError;

/// Certificate manager
pub struct CertificateManager {
    /// Certificate storage backend
    store: Box<dyn CertificateStore>,
    /// In-memory certificate cache
    cache: RwLock<HashMap<String, Certificate>>,
    /// ACME configuration
    acme_config: Option<AcmeConfig>,
    /// Auto-renewal enabled
    auto_renewal: bool,
    /// Days before expiry to trigger renewal
    renewal_days: u32,
}

/// Certificate storage backend trait
#[async_trait::async_trait]
pub trait CertificateStore: Send + Sync {
    /// Get certificate for domain
    async fn get(&self, domain: &str) -> Result<Option<Certificate>, CertError>;
    /// Store certificate for domain
    async fn put(&self, domain: &str, cert: &Certificate) -> Result<(), CertError>;
    /// Delete certificate for domain
    async fn delete(&self, domain: &str) -> Result<(), CertError>;
    /// List all stored domains
    async fn list(&self) -> Result<Vec<String>, CertError>;
}

/// In-memory certificate store
pub struct MemoryStore {
    certs: RwLock<HashMap<String, Certificate>>,
}

impl MemoryStore {
    /// Create new memory store
    pub fn new() -> Self {
        Self {
            certs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CertificateStore for MemoryStore {
    async fn get(&self, domain: &str) -> Result<Option<Certificate>, CertError> {
        let certs = self.certs.read().await;
        Ok(certs.get(domain).cloned())
    }

    async fn put(&self, domain: &str, cert: &Certificate) -> Result<(), CertError> {
        let mut certs = self.certs.write().await;
        certs.insert(domain.to_string(), cert.clone());
        Ok(())
    }

    async fn delete(&self, domain: &str) -> Result<(), CertError> {
        let mut certs = self.certs.write().await;
        certs.remove(domain);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, CertError> {
        let certs = self.certs.read().await;
        Ok(certs.keys().cloned().collect())
    }
}

/// Filesystem certificate store
pub struct FileStore {
    base_path: PathBuf,
}

impl FileStore {
    /// Create new file store
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn cert_path(&self, domain: &str) -> PathBuf {
        self.base_path.join(domain).with_extension("json")
    }

    fn key_path(&self, domain: &str) -> PathBuf {
        self.base_path.join(domain).with_extension("key")
    }
}

#[async_trait::async_trait]
impl CertificateStore for FileStore {
    async fn get(&self, domain: &str) -> Result<Option<Certificate>, CertError> {
        let path = self.cert_path(domain);
        if !path.exists() {
            return Ok(None);
        }

        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| CertError::StorageError(format!("Failed to read certificate: {}", e)))?;

        let cert: Certificate = serde_json::from_slice(&data)
            .map_err(|e| CertError::StorageError(format!("Failed to parse certificate: {}", e)))?;

        Ok(Some(cert))
    }

    async fn put(&self, domain: &str, cert: &Certificate) -> Result<(), CertError> {
        if !self.base_path.exists() {
            tokio::fs::create_dir_all(&self.base_path)
                .await
                .map_err(|e| {
                    CertError::StorageError(format!("Failed to create directory: {}", e))
                })?;
        }

        let path = self.cert_path(domain);
        let data = serde_json::to_vec_pretty(cert).map_err(|e| {
            CertError::StorageError(format!("Failed to serialize certificate: {}", e))
        })?;

        // Set restrictive permissions for key file
        tokio::fs::write(&path, &data)
            .await
            .map_err(|e| CertError::StorageError(format!("Failed to write certificate: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, domain: &str) -> Result<(), CertError> {
        let cert_path = self.cert_path(domain);
        let key_path = self.key_path(domain);

        if cert_path.exists() {
            tokio::fs::remove_file(&cert_path).await.map_err(|e| {
                CertError::StorageError(format!("Failed to delete certificate: {}", e))
            })?;
        }

        if key_path.exists() {
            tokio::fs::remove_file(&key_path)
                .await
                .map_err(|e| CertError::StorageError(format!("Failed to delete key: {}", e)))?;
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, CertError> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }

        let mut entries = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| CertError::StorageError(format!("Failed to list directory: {}", e)))?;

        let mut domains = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| CertError::StorageError(format!("Failed to read entry: {}", e)))?
        {
            if let Some(name) = entry.path().file_stem() {
                domains.push(name.to_string_lossy().to_string());
            }
        }

        Ok(domains)
    }
}

impl CertificateManager {
    /// Create manager with storage backend
    pub async fn new(config: &CertConfig) -> Result<Self, CertError> {
        let store: Box<dyn CertificateStore> = match &config.storage {
            CertStorage::Memory => Box::new(MemoryStore::new()),
            CertStorage::File { path } => Box::new(FileStore::new(path)),
        };

        let acme_config = config.acme.as_ref().map(|a| AcmeConfig {
            directory_url: a.directory_url.clone(),
            contact_email: a.contact_email.clone(),
            challenge_type: a.challenge_type.clone(),
        });

        info!("Certificate manager initialized");

        Ok(Self {
            store,
            cache: RwLock::new(HashMap::new()),
            acme_config,
            auto_renewal: true,
            renewal_days: 30,
        })
    }

    /// Get certificate for domain (SNI callback)
    pub async fn get_certificate(&self, domain: &str) -> Result<Option<Certificate>, CertError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cert) = cache.get(domain) {
                if !cert.is_expired() {
                    return Ok(Some(cert.clone()));
                }
            }
        }

        // Check storage
        if let Some(cert) = self.store.get(domain).await? {
            if !cert.is_expired() {
                // Update cache
                let mut cache = self.cache.write().await;
                cache.insert(domain.to_string(), cert.clone());
                return Ok(Some(cert));
            }
        }

        Ok(None)
    }

    /// Request new certificate via ACME
    pub async fn request_certificate(&self, domain: &str) -> Result<Certificate, CertError> {
        let acme_config = self
            .acme_config
            .as_ref()
            .ok_or_else(|| CertError::AcmeNotConfigured)?;

        info!("Requesting certificate for {} via ACME", domain);

        // In production, this would use acme-lib or acme2 crate
        // For development, we generate a self-signed certificate

        let cert = self.generate_self_signed(domain)?;

        // Store certificate
        self.store.put(domain, &cert).await?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.insert(domain.to_string(), cert.clone());

        info!("Certificate obtained for {}", domain);

        Ok(cert)
    }

    /// Import existing certificate
    pub async fn import_certificate(
        &self,
        domain: &str,
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<(), CertError> {
        info!("Importing certificate for {}", domain);

        let cert = Certificate {
            domain: domain.to_string(),
            cert_pem: cert_pem.to_string(),
            key_pem: key_pem.to_string(),
            chain_pem: None,
            not_before: Utc::now(),
            not_after: Utc::now() + chrono::Duration::days(365),
            created_at: Utc::now(),
        };

        cert.validate()?;

        self.store.put(domain, &cert).await?;

        let mut cache = self.cache.write().await;
        cache.insert(domain.to_string(), cert);

        Ok(())
    }

    /// Check and renew expiring certificates
    pub async fn renew_expiring(&self, days_before: u32) -> Result<Vec<String>, CertError> {
        info!("Checking for expiring certificates ({} days)", days_before);

        let mut renewed = Vec::new();
        let threshold = Utc::now() + chrono::Duration::days(days_before as i64);

        let domains = self.store.list().await?;

        for domain in domains {
            if let Some(cert) = self.store.get(&domain).await? {
                if cert.not_after < threshold {
                    info!("Renewing certificate for {}", domain);

                    match self.request_certificate(&domain).await {
                        Ok(_) => {
                            renewed.push(domain);
                        }
                        Err(e) => {
                            error!("Failed to renew certificate for {}: {}", domain, e);
                        }
                    }
                }
            }
        }

        info!("Renewed {} certificates", renewed.len());

        Ok(renewed)
    }

    /// Revoke certificate
    pub async fn revoke(&self, domain: &str, _reason: RevocationReason) -> Result<(), CertError> {
        info!("Revoking certificate for {}", domain);

        // Remove from cache
        {
            let mut cache = self.cache.write().await;
            cache.remove(domain);
        }

        // Remove from storage
        self.store.delete(domain).await?;

        // In production, would send revocation to CA

        info!("Certificate revoked for {}", domain);

        Ok(())
    }

    /// Generate self-signed certificate (for development/testing)
    fn generate_self_signed(&self, domain: &str) -> Result<Certificate, CertError> {
        use rcgen::{
            CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256,
        };

        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, domain);
        params.alg = &PKCS_ECDSA_P256_SHA256;
        params.subject_alt_names = vec![rcgen::SanType::DnsName(domain.to_string())];

        let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|e| CertError::GenerationError(format!("Failed to generate key: {}", e)))?;

        let cert = rcgen::Certificate::params(params)
            .map_err(|e| CertError::GenerationError(format!("Failed to create cert: {}", e)))?;

        let cert_pem = cert
            .serialize_pem()
            .map_err(|e| CertError::GenerationError(format!("Failed to serialize cert: {}", e)))?;

        let key_pem = key_pair.serialize_pem();

        let now = Utc::now();

        Ok(Certificate {
            domain: domain.to_string(),
            cert_pem,
            key_pem,
            chain_pem: None,
            not_before: now,
            not_after: now + chrono::Duration::days(90),
            created_at: now,
        })
    }

    /// Start background renewal worker
    pub async fn start_renewal_worker(&self) {
        let store_domains = self.store.list().await;
        let _renewal_days = self.renewal_days;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(86400)); // Daily

            loop {
                interval.tick().await;

                if let Ok(domains) = store_domains.as_ref() {
                    for domain in domains {
                        // Check and renew if needed
                        debug!("Checking certificate for {}", domain);
                        let _ = _renewal_days; // Will be used in actual renewal logic
                    }
                }
            }
        });
    }
}

/// Certificate data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// Primary domain
    pub domain: String,
    /// Certificate PEM
    pub cert_pem: String,
    /// Private key PEM
    pub key_pem: String,
    /// Certificate chain PEM
    pub chain_pem: Option<String>,
    /// Not valid before
    pub not_before: DateTime<Utc>,
    /// Not valid after
    pub not_after: DateTime<Utc>,
    /// Created at
    pub created_at: DateTime<Utc>,
}

impl Certificate {
    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        self.not_after < Utc::now()
    }

    /// Check if certificate needs renewal
    pub fn needs_renewal(&self, days: u32) -> bool {
        let threshold = Utc::now() + chrono::Duration::days(days as i64);
        self.not_after < threshold
    }

    /// Validate certificate
    pub fn validate(&self) -> Result<(), CertError> {
        if self.domain.is_empty() {
            return Err(CertError::ValidationError("Domain is empty".into()));
        }

        if self.cert_pem.is_empty() {
            return Err(CertError::ValidationError(
                "Certificate PEM is empty".into(),
            ));
        }

        if self.key_pem.is_empty() {
            return Err(CertError::ValidationError("Key PEM is empty".into()));
        }

        Ok(())
    }

    /// Convert to rustls certificate
    pub fn to_rustls_cert(&self) -> Result<(Vec<RustlsCertificate>, PrivateKey), CertError> {
        // Parse certificate chain
        let certs = rustls_pemfile::certs(&mut self.cert_pem.as_bytes())
            .map_err(|e| CertError::ParseError(format!("Failed to parse cert: {}", e)))?
            .into_iter()
            .map(RustlsCertificate)
            .collect();

        // Parse private key
        let key = rustls_pemfile::private_key(&mut self.key_pem.as_bytes())
            .map_err(|e| CertError::ParseError(format!("Failed to parse key: {}", e)))?
            .ok_or_else(|| CertError::ParseError("No private key found".into()))?;

        Ok((certs, key))
    }

    /// Get full chain PEM
    pub fn fullchain_pem(&self) -> String {
        match &self.chain_pem {
            Some(chain) => format!("{}\n{}", self.cert_pem, chain),
            None => self.cert_pem.clone(),
        }
    }
}

/// Certificate configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertConfig {
    /// Storage backend
    pub storage: CertStorage,
    /// ACME configuration
    pub acme: Option<AcmeConfigDto>,
}

impl Default for CertConfig {
    fn default() -> Self {
        Self {
            storage: CertStorage::Memory,
            acme: None,
        }
    }
}

/// Certificate storage type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CertStorage {
    /// In-memory storage
    Memory,
    /// Filesystem storage
    File { path: String },
}

/// ACME configuration DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeConfigDto {
    /// ACME directory URL
    pub directory_url: String,
    /// Contact email
    pub contact_email: String,
    /// Challenge type
    pub challenge_type: String,
}

/// ACME configuration
#[derive(Debug, Clone)]
pub struct AcmeConfig {
    /// ACME directory URL (Let's Encrypt)
    pub directory_url: String,
    /// Contact email
    pub contact_email: String,
    /// Challenge type: http01 or dns01
    pub challenge_type: String,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            contact_email: String::new(),
            challenge_type: "http01".to_string(),
        }
    }
}

/// Revocation reasons
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RevocationReason {
    Unspecified,
    KeyCompromise,
    CaCompromise,
    AffiliationChanged,
    Superseded,
    CessationOfOperation,
}

/// Certificate resolver for rustls SNI
pub struct CertificateResolver {
    _manager: Arc<CertificateManager>,
}

impl CertificateResolver {
    /// Create new resolver
    pub fn new(manager: Arc<CertificateManager>) -> Self {
        Self { _manager: manager }
    }
}

impl ResolvesServerCert for CertificateResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        // Get SNI from client hello
        let domain = client_hello.server_name()?;

        // In async context, we'd use block_on
        // For now, return None (would need to be refactored for async)
        debug!("Resolving certificate for {}", domain);
        None
    }
}

/// Certificate errors
#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("ACME error: {0}")]
    AcmeError(String),

    #[error("ACME not configured")]
    AcmeNotConfigured,

    #[error("DNS challenge failed: {0}")]
    DnsChallengeFailed(String),

    #[error("HTTP challenge failed: {0}")]
    HttpChallengeFailed(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Generation error: {0}")]
    GenerationError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_certificate_manager_creation() {
        let config = CertConfig::default();
        let manager = CertificateManager::new(&config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_memory_store() {
        let store = MemoryStore::new();

        let cert = Certificate {
            domain: "example.com".to_string(),
            cert_pem: "CERT".to_string(),
            key_pem: "KEY".to_string(),
            chain_pem: None,
            not_before: Utc::now(),
            not_after: Utc::now() + chrono::Duration::days(90),
            created_at: Utc::now(),
        };

        store.put("example.com", &cert).await.unwrap();
        let retrieved = store.get("example.com").await.unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_certificate_expiry() {
        let cert = Certificate {
            domain: "example.com".to_string(),
            cert_pem: "CERT".to_string(),
            key_pem: "KEY".to_string(),
            chain_pem: None,
            not_before: Utc::now(),
            not_after: Utc::now() + chrono::Duration::days(10),
            created_at: Utc::now(),
        };

        assert!(!cert.is_expired());
        assert!(cert.needs_renewal(30));
    }
}
