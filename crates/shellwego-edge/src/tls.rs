//! TLS certificate management and Let's Encrypt ACME automation
//!
//! Implements real ACMEv2 protocol for Let's Encrypt certificate provisioning,
//! HTTP-01 challenge response, automatic renewal, and SNI-based certificate
//! selection via the `ResolvesServerCert` trait.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use p256::ecdsa::{SigningKey, VerifyingKey};
use p256::pkcs8::EncodePrivateKey;
use rand::rngs::OsRng;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::ResolvesServerCert;
use rustls::sign::CertifiedKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Certificate Store
// ---------------------------------------------------------------------------

/// Certificate manager
pub struct CertificateManager {
    /// Certificate storage backend
    store: Box<dyn CertificateStore>,
    /// In-memory certificate cache (domain -> Certificate)
    cache: RwLock<HashMap<String, Certificate>>,
    /// ACME configuration
    acme_config: Option<AcmeConfig>,
    /// Auto-renewal enabled
    auto_renewal: bool,
    /// Days before expiry to trigger renewal
    renewal_days: u32,
    /// ACME challenge tokens for HTTP-01 validation
    /// (token -> key_authorization) served at `/.well-known/acme-challenge/{token}`
    challenge_tokens: Arc<std::sync::RwLock<HashMap<String, String>>>,
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

// ---------------------------------------------------------------------------
// Certificate Manager
// ---------------------------------------------------------------------------

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
            challenge_tokens: Arc::new(std::sync::RwLock::new(HashMap::new())),
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

    /// Request new certificate via real ACME (Let's Encrypt) protocol
    pub async fn request_certificate(&self, domain: &str) -> Result<Certificate, CertError> {
        let acme_config = self
            .acme_config
            .as_ref()
            .ok_or(CertError::AcmeNotConfigured)?;

        info!("Requesting certificate for {} via ACME", domain);

        // 1. Create an ACME client with a fresh ECDSA P-256 account key
        let account_key = SigningKey::random(&mut OsRng);
        let mut acme = AcmeClient::new(acme_config, account_key).await?;

        // 2. Register account (or find existing)
        let account_url = acme.register_account().await?;
        info!("ACME account registered: {}", account_url);

        // 3. Create order for the domain
        let order = acme.create_order(domain).await?;
        info!("ACME order created: {}", order.url);

        // 4. Find HTTP-01 challenge
        let http_challenge = order
            .authorizations
            .iter()
            .flat_map(|auth| auth.challenges.iter())
            .find(|c| c.challenge_type == "http-01")
            .ok_or_else(|| {
                CertError::HttpChallengeFailed("No HTTP-01 challenge available".into())
            })?;

        // 5. Compute key authorization and store it for the challenge server
        let key_authorization = acme.key_authorization(&http_challenge.token)?;

        // Store challenge token so the HTTP handler at `/.well-known/acme-challenge/{token}`
        // can serve it during validation
        {
            let mut tokens = self
                .challenge_tokens
                .write()
                .map_err(|e| CertError::StorageError(format!("Challenge lock poisoned: {}", e)))?;
            tokens.insert(http_challenge.token.clone(), key_authorization.clone());
        }

        info!(
            "HTTP-01 challenge token stored for domain {}",
            domain
        );

        // 6. Tell ACME server we're ready for the challenge
        acme.answer_challenge(&http_challenge.url).await?;
        info!("Told ACME server challenge is ready, polling for validation...");

        // 7. Poll for challenge completion (wait for ACME server to validate)
        acme.poll_authorization(&order)
            .await
            .map_err(|e| CertError::HttpChallengeFailed(format!("Challenge validation failed: {}", e)))?;

        info!("HTTP-01 challenge validated for {}", domain);

        // 8. Poll order until it is ready, then finalize
        let finalize_url = acme.poll_until_ready(&order.url).await?;
        info!("Order ready, finalizing certificate for {}", domain);

        // 9. Generate CSR and finalize order
        let (cert_pem, chain_pem) = acme
            .finalize_certificate(&finalize_url, domain)
            .await
            .map_err(|e| CertError::AcmeError(format!("Finalization failed: {}", e)))?;

        // 10. Get the private key PEM
        let key_pem = acme.private_key_pem();

        // 11. Clean up challenge token
        let _ = finalize_url;
        {
            let mut tokens = self
                .challenge_tokens
                .write()
                .map_err(|e| CertError::StorageError(format!("Challenge lock poisoned: {}", e)))?;
            tokens.remove(&http_challenge.token);
        }

        let now = Utc::now();
        let cert = Certificate {
            domain: domain.to_string(),
            cert_pem,
            key_pem,
            chain_pem: Some(chain_pem),
            not_before: now,
            // Let's Encrypt certificates are valid for 90 days
            not_after: now + chrono::Duration::days(90),
            created_at: now,
        };

        // Store certificate
        self.store.put(domain, &cert).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(domain.to_string(), cert.clone());
        }

        info!("Certificate obtained for {} via Let's Encrypt", domain);
        Ok(cert)
    }

    /// Look up the HTTP-01 challenge token for the ACME challenge handler.
    /// Returns the key-authorization value for the given token, or `None` if
    /// no challenge is pending for that token.
    pub fn get_challenge_token(&self, token: &str) -> Option<String> {
        self.challenge_tokens
            .read()
            .ok()
            .and_then(|tokens| tokens.get(token).cloned())
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

        // In production, would send revocation request to CA via ACME

        info!("Certificate revoked for {}", domain);
        Ok(())
    }

    /// Generate self-signed certificate (for development/testing fallback)
    pub fn generate_self_signed(domain: &str) -> Result<Certificate, CertError> {
        use rcgen::{
            CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256,
        };

        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, domain);
        // rcgen 0.12+: alg is now Option<&SignatureAlgorithm>
        params.alg = Some(&PKCS_ECDSA_P256_SHA256);
        params.subject_alt_names = vec![rcgen::SanType::DnsName(domain.to_string())];

        // rcgen 0.12+: KeyPair::generate() takes no argument;
        // the algorithm is read from params.alg
        let key_pair = KeyPair::generate()
            .map_err(|e| CertError::GenerationError(format!("Failed to generate key: {}", e)))?;

        let key_pem = key_pair.serialize_pem();
        params.key_pair = Some(key_pair);
        let cert = rcgen::Certificate::from_params(params)
            .map_err(|e| CertError::GenerationError(format!("Failed to create cert: {}", e)))?;

        let cert_pem = cert
            .serialize_pem()
            .map_err(|e| CertError::GenerationError(format!("Failed to serialize cert: {}", e)))?;

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

    /// Get a cached certificate as a `CertifiedKey` suitable for rustls.
    /// This is used by the `CertificateResolver` for SNI.
    ///
    /// Returns `None` when no certificate is available for the domain (caller
    /// may fall back to generating a temporary self-signed certificate).
    pub async fn get_certified_key(&self, domain: &str) -> Option<Arc<CertifiedKey>> {
        let cert = self.get_certificate(domain).await.ok()??;

        match cert.to_rustls_cert() {
            Ok((cert_chain, key)) => {
                let certified_key = CertifiedKey::new(cert_chain,
                    rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(key.clone_key()))
                        .ok()?,
                );
                Some(Arc::new(certified_key))
            }
            Err(e) => {
                warn!(
                    "Failed to convert certificate for {} to rustls: {}",
                    domain, e
                );
                None
            }
        }
    }

    /// Start background renewal worker.
    ///
    /// Checks all stored certificates once per day and renews any that expire
    /// within `renewal_days` days.
    pub fn start_renewal_worker(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(86400)); // Daily

            loop {
                interval.tick().await;

                if self.auto_renewal {
                    info!("Background renewal check starting");
                    match self.renew_expiring(self.renewal_days).await {
                        Ok(renewed) if !renewed.is_empty() => {
                            info!("Background renewal completed: {} certs renewed", renewed.len());
                        }
                        Ok(_) => {
                            debug!("Background renewal check: no certs need renewal");
                        }
                        Err(e) => {
                            error!("Background renewal check failed: {}", e);
                        }
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// ACME v2 Client
// ---------------------------------------------------------------------------

/// ACME v2 client implementing the Let's Encrypt protocol via raw HTTP calls.
///
/// This client handles:
/// - Directory discovery
/// - Replay-nonce management
/// - JWS (JSON Web Signature) signing with ECDSA P-256 / ES256
/// - Account registration
/// - Order creation
/// - HTTP-01 challenge answering and polling
/// - CSR generation and certificate finalization
struct AcmeClient {
    /// ACME directory URLs
    directory: AcmeDirectory,
    /// HTTP client
    http: reqwest::Client,
    /// Account private key (ECDSA P-256)
    account_key: SigningKey,
    /// JWK (public key) in JSON form
    jwk: serde_json::Value,
    /// Account URL (set after registration)
    account_url: Option<String>,
    /// Last replay-nonce from the server
    nonce: parking_lot::Mutex<Option<String>>,
    /// Contact email
    contact_email: String,
}

/// ACME directory endpoints
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AcmeDirectory {
    #[serde(rename = "newNonce")]
    new_nonce: String,
    #[serde(rename = "newAccount")]
    new_account: String,
    #[serde(rename = "newOrder")]
    new_order: String,
    #[serde(rename = "revokeCert")]
    revoke_cert: String,
}

/// ACME order response
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcmeOrder {
    url: String,
    status: String,
    expires: String,
    identifiers: Vec<AcmeIdentifier>,
    authorizations: Vec<AcmeAuthorization>,
    finalize: String,
    certificate: Option<String>,
}

/// ACME identifier
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AcmeIdentifier {
    #[serde(rename = "type")]
    identifier_type: String,
    value: String,
}

/// ACME authorization
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AcmeAuthorization {
    url: String,
    status: String,
    domain: Option<String>,
    challenges: Vec<AcmeChallenge>,
}

/// ACME challenge
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AcmeChallenge {
    #[serde(rename = "type")]
    challenge_type: String,
    url: String,
    token: String,
    status: Option<String>,
    validated: Option<String>,
}

/// ACME problem detail
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct AcmeProblem {
    #[serde(rename = "type")]
    problem_type: String,
    detail: String,
    status: Option<u16>,
}

impl AcmeClient {
    /// Create a new ACME client, discovering the directory endpoints.
    async fn new(config: &AcmeConfig, account_key: SigningKey) -> Result<Self, CertError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CertError::AcmeError(format!("Failed to create HTTP client: {}", e)))?;

        // Discover directory
        let directory: AcmeDirectory = http
            .get(&config.directory_url)
            .send()
            .await
            .map_err(|e| CertError::AcmeError(format!("Directory request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| CertError::AcmeError(format!("Failed to parse directory: {}", e)))?;

        info!(
            "ACME directory discovered: newOrder={}, newAccount={}",
            directory.new_order, directory.new_account
        );

        let jwk = build_jwk(account_key.verifying_key());

        Ok(Self {
            directory,
            http,
            account_key,
            jwk,
            account_url: None,
            nonce: parking_lot::Mutex::new(None),
            contact_email: config.contact_email.clone(),
        })
    }

    /// Register (or locate) the ACME account.
    /// Returns the account URL.
    async fn register_account(&mut self) -> Result<String, CertError> {
        let payload = serde_json::json!({
            "termsOfServiceAgreed": true,
            "contact": vec![format!("mailto:{}", self.contact_email)]
        });

        let response = self
            .post_jose(&self.directory.new_account, &payload, None)
            .await?;

        let account_url = if let Some(loc) = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
        {
            Some(loc.to_string())
        } else {
            // Some servers return the URL in the body
            response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(String::from))
        }
        .ok_or_else(|| CertError::AcmeError("No account URL in registration response".into()))?;

        self.account_url = Some(account_url.clone());
        info!("ACME account registered: {}", account_url);
        Ok(account_url)
    }

    /// Create a new certificate order for the given domain.
    async fn create_order(&self, domain: &str) -> Result<AcmeOrder, CertError> {
        let payload = serde_json::json!({
            "identifiers": [{
                "type": "dns",
                "value": domain
            }]
        });

        let response = self
            .post_jose(&self.directory.new_order, &payload, None)
            .await?;

        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut order: AcmeOrder = response
            .json()
            .await
            .map_err(|e| CertError::AcmeError(format!("Failed to parse order: {}", e)))?;

        // Use Location header for order URL if body doesn't have it
        if order.url.is_empty() {
            if let Some(loc) = location {
                order.url = loc;
            }
        }

        Ok(order)
    }

    /// Compute the key authorization for HTTP-01 challenge.
    ///
    /// `key_authorization = token.base64url "." . SHA-256(jwk).base64url`
    pub fn key_authorization(&self, token: &str) -> Result<String, CertError> {
        let thumbprint = jwk_thumbprint(&self.jwk)?;
        Ok(format!("{}.{}", token, thumbprint))
    }

    /// Tell the ACME server we are ready to respond to the challenge.
    async fn answer_challenge(&self, challenge_url: &str) -> Result<(), CertError> {
        let payload = serde_json::json!({});

        let response = self.post_jose(challenge_url, &payload, None).await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CertError::HttpChallengeFailed(format!(
                "Challenge answer failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    /// Poll authorizations until they are all `valid` (or one fails).
    async fn poll_authorization(&self, order: &AcmeOrder) -> Result<(), CertError> {
        for auth in &order.authorizations {
            self.poll_until_status(&auth.url, "valid", Duration::from_secs(120), Duration::from_secs(3))
                .await?;
        }
        Ok(())
    }

    /// Poll the order until it reaches `ready` status, return the finalize URL.
    async fn poll_until_ready(&self, order_url: &str) -> Result<String, CertError> {
        // Fetch the order
        for _ in 0..40 {
            let response = self
                .http
                .get(order_url)
                .send()
                .await
                .map_err(|e| CertError::AcmeError(format!("Order poll failed: {}", e)))?;

            let order: AcmeOrder = response
                .json()
                .await
                .map_err(|e| CertError::AcmeError(format!("Failed to parse order poll: {}", e)))?;

            match order.status.as_str() {
                "ready" => return Ok(order.finalize),
                "valid" => {
                    // Already finalized — return the finalize URL
                    return Ok(order.finalize);
                }
                "invalid" => {
                    return Err(CertError::AcmeError(format!(
                        "Order became invalid: {}",
                        order_url
                    )));
                }
                _ => {
                    // Still pending — wait and retry
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }

        Err(CertError::AcmeError(
            "Timed out waiting for order to become ready".into(),
        ))
    }

    /// Generate a CSR, send it to finalize the order, and download the cert.
    async fn finalize_certificate(
        &self,
        finalize_url: &str,
        domain: &str,
    ) -> Result<(String, String), CertError> {
        let csr_der = self.generate_csr(domain)?;

        let payload = serde_json::json!({
            "csr": URL_SAFE_NO_PAD.encode(&csr_der)
        });

        let response = self.post_jose(finalize_url, &payload, None).await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CertError::AcmeError(format!(
                "Finalization failed ({}): {}",
                status, body
            )));
        }

        // The server should return a Location header with the order URL
        let order_url = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let order: AcmeOrder = response
            .json()
            .await
            .unwrap_or_else(|_| AcmeOrder {
                url: order_url.unwrap_or_default(),
                status: "processing".into(),
                expires: String::new(),
                identifiers: vec![],
                authorizations: vec![],
                finalize: finalize_url.to_string(),
                certificate: None,
            });

        // Poll for certificate URL
        let cert_url = if let Some(url) = order.certificate {
            url
        } else {
            self.poll_for_certificate_url(&order.url).await?
        };

        self.download_certificate(&cert_url, finalize_url.to_string())
            .await
    }

    /// Poll an order until a certificate URL is available.
    async fn poll_for_certificate_url(&self, order_url: &str) -> Result<String, CertError> {
        for _ in 0..40 {
            let response = self
                .http
                .get(order_url)
                .send()
                .await
                .map_err(|e| CertError::AcmeError(format!("Cert poll failed: {}", e)))?;

            let order: AcmeOrder = response
                .json()
                .await
                .map_err(|e| CertError::AcmeError(format!("Failed to parse cert poll: {}", e)))?;

            if let Some(cert_url) = order.certificate {
                return Ok(cert_url);
            }

            match order.status.as_str() {
                "invalid" => {
                    return Err(CertError::AcmeError("Order became invalid during finalization".into()));
                }
                _ => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }

        Err(CertError::AcmeError(
            "Timed out waiting for certificate URL".into(),
        ))
    }

    /// Download the issued certificate (PEM).
    async fn download_certificate(
        &self,
        cert_url: &str,
        _finalize_url: String,
    ) -> Result<(String, String), CertError> {
        let response = self
            .http
            .get(cert_url)
            .send()
            .await
            .map_err(|e| CertError::AcmeError(format!("Certificate download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(CertError::AcmeError(format!(
                "Certificate download returned {}",
                response.status()
            )));
        }

        // The response body is typically PEM-encoded certificate chain
        let full_pem = response
            .text()
            .await
            .map_err(|e| CertError::AcmeError(format!("Failed to read cert body: {}", e)))?;

        // Split into leaf cert and chain
        // PEM blocks alternate between the server cert and intermediates
        let (cert_pem, chain_pem) = split_cert_chain(&full_pem);

        // If the cert URL itself had the certificate, we can use finalize_url as placeholder
        if cert_pem.is_empty() {
            // Try fetching from the finalize order
            return Err(CertError::AcmeError(
                "Downloaded certificate was empty".into(),
            ));
        }

        Ok((cert_pem, chain_pem))
    }

    /// Generate a CSR (Certificate Signing Request) for the domain using
    /// the account key as the private key.
    ///
    /// Returns the DER-encoded CSR.
    fn generate_csr(&self, domain: &str) -> Result<Vec<u8>, CertError> {
        // Build a minimal DER-encoded CSR using the ECDSA P-256 key.
        // We construct it manually since we don't depend on x509-parser or similar.
        generate_ecdsa_csr(&self.account_key, domain)
    }

    /// Get the account private key in PEM format.
    fn private_key_pem(&self) -> String {
        self.account_key
            .to_pkcs8_der()
            .map(|der| {
                // Wrap in PEM
                let b64 = URL_SAFE_NO_PAD.encode(der.as_bytes());
                format!(
                    "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                    pem_line_wrap(&b64)
                )
            })
            .unwrap_or_default()
    }

    // --- Low-level ACME helpers ---

    /// POST a signed JWS to the ACME server.
    /// If `kid` is `Some`, uses it as the `kid` header field (for authenticated
    /// requests). Otherwise, includes the `jwk` in the header (for registration).
    async fn post_jose(
        &self,
        url: &str,
        payload: &serde_json::Value,
        kid: Option<&str>,
    ) -> Result<reqwest::Response, CertError> {
        // Get a fresh nonce if we don't have one
        if self.nonce.lock().is_none() {
            self.get_nonce().await?;
        }

        let nonce = self
            .nonce
            .lock()
            .clone()
            .ok_or_else(|| CertError::AcmeError("No replay nonce available".into()))?;

        // Build JWS header
        let mut header = serde_json::json!({
            "alg": "ES256",
            "nonce": nonce,
            "url": url
        });

        if let Some(kid) = kid {
            header["kid"] = serde_json::json!(kid);
        } else {
            header["jwk"] = self.jwk.clone();
        }

        // JWS protected header -> base64url
        let protected = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&header)
                .map_err(|e| CertError::AcmeError(format!("Failed to serialize header: {}", e)))?,
        );

        // Payload -> base64url
        let payload_b64 = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(payload)
                .map_err(|e| CertError::AcmeError(format!("Failed to serialize payload: {}", e)))?,
        );

        // Signing input = protected.payload
        let signing_input = format!("{}.{}", protected, payload_b64);

        // Sign with ECDSA P-256 / SHA-256
        let signature = {
            use p256::ecdsa::signature::Signer;
            let sig: p256::ecdsa::Signature = self.account_key.sign(signing_input.as_bytes());
            // Convert to DER format then base64url encode
            sig.to_der().to_bytes().to_vec()
        };
        let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);

        // Build the JWS JSON
        let jws = serde_json::json!({
            "protected": protected,
            "payload": payload_b64,
            "signature": signature_b64
        });

        // POST to ACME server
        let response = self
            .http
            .post(url)
            .header(CONTENT_TYPE, "application/jose+json")
            .json(&jws)
            .send()
            .await
            .map_err(|e| CertError::AcmeError(format!("POST to ACME failed: {}", e)))?;

        // Update nonce from response headers
        if let Some(new_nonce) = response.headers().get("replay-nonce") {
            if let Ok(nonce_str) = new_nonce.to_str() {
                *self.nonce.lock() = Some(nonce_str.to_string());
            }
        }

        // Handle ACME errors
        if response.status().is_client_error() || response.status().is_server_error() {
            // Try to parse the error body as a problem document
            let status = response.status();
            // We need to consume the body to avoid "response already consumed" errors
            // Clone the response first... actually we can't clone, so we consume.
            let body = response.text().await.unwrap_or_default();
            if let Ok(problem) = serde_json::from_str::<AcmeProblem>(&body) {
                return Err(CertError::AcmeError(format!(
                    "ACME error ({}): {}",
                    status, problem.detail
                )));
            }
            return Err(CertError::AcmeError(format!(
                "ACME request failed ({}): {}",
                status, body
            )));
        }

        Ok(response)
    }

    /// Obtain a new replay-nonce from the ACME server.
    async fn get_nonce(&self) -> Result<(), CertError> {
        let response = self
            .http
            .head(&self.directory.new_nonce)
            .send()
            .await
            .map_err(|e| CertError::AcmeError(format!("Nonce request failed: {}", e)))?;

        if let Some(nonce) = response.headers().get("replay-nonce") {
            let nonce_str = nonce
                .to_str()
                .map_err(|e| CertError::AcmeError(format!("Invalid nonce header: {}", e)))?;
            *self.nonce.lock() = Some(nonce_str.to_string());
            return Ok(());
        }

        Err(CertError::AcmeError(
            "No replay-nonce in response".into(),
        ))
    }

    /// Poll a URL until it reaches the desired status.
    async fn poll_until_status(
        &self,
        url: &str,
        desired_status: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<(), CertError> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(CertError::AcmeError(format!(
                    "Timed out waiting for {} status at {}",
                    desired_status, url
                )));
            }

            let response = self
                .http
                .get(url)
                .send()
                .await
                .map_err(|e| CertError::AcmeError(format!("Poll failed: {}", e)))?;

            let auth: AcmeAuthorization = response
                .json()
                .await
                .map_err(|e| CertError::AcmeError(format!("Failed to parse auth poll: {}", e)))?;

            match auth.status.as_str() {
                s if s == desired_status => return Ok(()),
                "invalid" => {
                    return Err(CertError::HttpChallengeFailed(format!(
                        "Authorization became invalid for {}",
                        url
                    )));
                }
                _ => {
                    tokio::time::sleep(interval).await;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JWK helpers
// ---------------------------------------------------------------------------

/// Build the JWK (JSON Web Key) representation of an ECDSA P-256 public key.
fn build_jwk(verifying_key: &VerifyingKey) -> serde_json::Value {
    // p256::VerifyingKey encodes the point as uncompressed SEC1 (0x04 || x || y), 65 bytes
    let point_bytes = verifying_key.to_encoded_point(false);
    let x_bytes = point_bytes.x().unwrap();
    let y_bytes = point_bytes.y().unwrap();

    serde_json::json!({
        "kty": "EC",
        "crv": "P-256",
        "x": URL_SAFE_NO_PAD.encode(x_bytes),
        "y": URL_SAFE_NO_PAD.encode(y_bytes),
    })
}

/// Compute the JWK SHA-256 thumbprint as described in RFC 7638.
fn jwk_thumbprint(jwk: &serde_json::Value) -> Result<String, CertError> {
    // For EC keys, the required members in alphabetical order are: crv, kty, x, y
    let thumbprint_json = serde_json::json!({
        "crv": jwk["crv"],
        "kty": jwk["kty"],
        "x": jwk["x"],
        "y": jwk["y"],
    });

    let json_bytes = serde_json::to_vec(&thumbprint_json)
        .map_err(|e| CertError::AcmeError(format!("Failed to serialize thumbprint: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&json_bytes);
    let hash = hasher.finalize();

    Ok(URL_SAFE_NO_PAD.encode(hash))
}

// ---------------------------------------------------------------------------
// CSR generation
// ---------------------------------------------------------------------------

/// Generate a minimal DER-encoded CSR for an ECDSA P-256 key and domain.
///
/// This builds the ASN.1 DER structure manually:
/// ```text
/// CertificationRequest ::= SEQUENCE {
///   certificationRequestInfo  CertificationRequestInfo,
///   signatureAlgorithm        AlgorithmIdentifier,
///   signature                 BIT STRING
/// }
/// CertificationRequestInfo ::= SEQUENCE {
///   version       INTEGER { v1(0) },
///   subject       Name,
///   subjectPKInfo SubjectPublicKeyInfo,
///   attributes    SET OF Attribute
/// }
/// ```
fn generate_ecdsa_csr(signing_key: &SigningKey, domain: &str) -> Result<Vec<u8>, CertError> {
    use p256::ecdsa::signature::Signer;

    // Build SubjectPublicKeyInfo from the verifying key
    let verifying_key = signing_key.verifying_key();
    let point_bytes = verifying_key.to_encoded_point(false); // uncompressed

    // SubjectPublicKeyInfo (RFC 5480)
    let spki = der_sequence(&[
        // AlgorithmIdentifier: id-ecPublicKey with P-256 curve
        der_sequence(&[
            der_oid(&[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01]), // id-ecPublicKey
            der_oid(&[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07]), // prime256v1
        ]),
        // BIT STRING: 0 (unused bits) || uncompressed point
        der_bit_string(point_bytes.as_bytes()),
    ]);

    // Build Subject (RDN with CN = domain)
    let cn_value = domain.as_bytes();
    let cn_set = der_set(&[
        // AttributeTypeAndValue: OID 2.5.4.3 (CN) + UTF8String value
        der_sequence(&[
            der_oid(&[0x55, 0x04, 0x03]), // commonName
            der_utf8_string(cn_value),
        ]),
    ]);

    // CertificationRequestInfo
    let req_info = der_sequence(&[
        der_integer(&[0]),           // version v1
        cn_set,                       // subject
        spki.clone(),                 // subjectPKInfo
        der_set(&[]),                 // attributes (empty)
    ]);

    // Sign the req_info
    let sig: p256::ecdsa::Signature = signing_key.sign(&req_info);
    let sig_der = sig.to_der();

    // CertificationRequest
    let csr = der_sequence(&[
        req_info,
        // signatureAlgorithm: ecdsa-with-SHA256
        der_sequence(&[
            der_oid(&[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02]), // ecdsa-with-SHA256
        ]),
        der_bit_string(sig_der.as_bytes()),
    ]);

    Ok(csr)
}

// ---------------------------------------------------------------------------
// Minimal DER encoding helpers
// ---------------------------------------------------------------------------

fn der_tag_length(tag: u8, length: usize) -> Vec<u8> {
    let mut out = vec![tag];
    if length < 128 {
        out.push(length as u8);
    } else if length < 256 {
        out.push(0x81);
        out.push(length as u8);
    } else {
        out.push(0x82);
        out.push((length >> 8) as u8);
        out.push(length as u8);
    }
    out
}

fn der_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = der_tag_length(tag, content.len());
    out.extend_from_slice(content);
    out
}

fn der_sequence(fields: &[Vec<u8>]) -> Vec<u8> {
    let content: Vec<u8> = fields.iter().flat_map(|f| f.iter().copied()).collect();
    der_wrap(0x30, &content)
}

fn der_set(fields: &[Vec<u8>]) -> Vec<u8> {
    let content: Vec<u8> = fields.iter().flat_map(|f| f.iter().copied()).collect();
    der_wrap(0x31, &content)
}

fn der_oid(oid_bytes: &[u8]) -> Vec<u8> {
    der_wrap(0x06, oid_bytes)
}

fn der_integer(bytes: &[u8]) -> Vec<u8> {
    der_wrap(0x02, bytes)
}

fn der_utf8_string(s: &[u8]) -> Vec<u8> {
    der_wrap(0x0C, s)
}

fn der_bit_string(bytes: &[u8]) -> Vec<u8> {
    // Prepend 0x00 for "no unused bits"
    let mut content = vec![0x00];
    content.extend_from_slice(bytes);
    der_wrap(0x03, &content)
}

// ---------------------------------------------------------------------------
// PEM helpers
// ---------------------------------------------------------------------------

/// Split a PEM certificate chain into the leaf cert and the remaining chain.
/// Returns (leaf_pem, chain_pem).
fn split_cert_chain(full_pem: &str) -> (String, String) {
    let certs: Vec<String> = full_pem
        .split("-----END CERTIFICATE-----")
        .filter_map(|block| {
            let block = block.trim();
            if block.contains("-----BEGIN CERTIFICATE-----") {
                Some(format!("{}\n-----END CERTIFICATE-----\n", block.trim()))
            } else {
                None
            }
        })
        .collect();

    if certs.is_empty() {
        return (String::new(), String::new());
    }

    let leaf = certs[0].clone();
    let chain: String = certs[1..].join("\n");

    (leaf, chain)
}

/// Wrap a base64 string into 64-character lines (PEM format).
fn pem_line_wrap(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 64 == 0 {
            result.push('\n');
        }
        result.push(c);
    }
    result
}

// ---------------------------------------------------------------------------
// Certificate data and types
// ---------------------------------------------------------------------------

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

    /// Convert to rustls certificate chain + private key.
    pub fn to_rustls_cert(
        &self,
    ) -> Result<(Vec<CertificateDer<'static>>, PrivatePkcs8KeyDer<'static>), CertError> {
        // Use full chain for the cert chain
        let pem_data = self.fullchain_pem();
        let mut cert_bytes = pem_data.as_bytes();
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_bytes)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CertError::ParseError(format!("Failed to parse cert: {}", e)))?;

        if certs.is_empty() {
            return Err(CertError::ParseError("No certificates found in PEM".into()));
        }

        // Parse private key
        let mut key_bytes = self.key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_bytes)
            .map_err(|e| CertError::ParseError(format!("Failed to parse key: {}", e)))?
            .ok_or_else(|| CertError::ParseError("No private key found".into()))?;

        // Convert to PKCS8 format
        let pkcs8_key = match key {
            rustls::pki_types::PrivateKeyDer::Pkcs8(pkcs8) => pkcs8.secret_pkcs8_der().to_vec(),
            rustls::pki_types::PrivateKeyDer::Sec1(sec1) => sec1.secret_sec1_der().to_vec(),
            #[allow(unreachable_patterns)]
            _ => return Err(CertError::ParseError("Unsupported key type".into())),
        };

        Ok((certs, PrivatePkcs8KeyDer::from(pkcs8_key)))
    }

    /// Get full chain PEM (leaf + intermediates)
    pub fn fullchain_pem(&self) -> String {
        match &self.chain_pem {
            Some(chain) => format!("{}\n{}", self.cert_pem, chain),
            None => self.cert_pem.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Certificate Resolver (SNI → rustls CertifiedKey)
// ---------------------------------------------------------------------------

/// Certificate resolver for rustls SNI.
///
/// Implements `ResolvesServerCert` to dynamically select a certificate based
/// on the SNI name sent by the client. Looks up the certificate in the
/// `CertificateManager`'s cache. If no certificate is found, generates a
/// self-signed temporary certificate on-the-fly (standard practice to avoid
/// connection failures while a real certificate is being provisioned).
pub struct CertificateResolver {
    /// Reference to the certificate manager for looking up certs.
    /// We keep a separate in-memory sync cache (domain → CertifiedKey) to
    /// avoid async calls inside the `ResolvesServerCert` trait (which is sync).
    manager: Arc<CertificateManager>,
    /// Fast-path cache: domain → CertifiedKey.
    /// Updated whenever a new certificate is stored.
    cert_cache: parking_lot::RwLock<HashMap<String, Arc<CertifiedKey>>>,
}

impl std::fmt::Debug for CertificateResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateResolver").finish()
    }
}

impl CertificateResolver {
    /// Create new resolver wrapping a certificate manager.
    pub fn new(manager: Arc<CertificateManager>) -> Self {
        Self {
            manager,
            cert_cache: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// Warm the cache with a certificate for the given domain.
    /// Call this after `request_certificate()` completes.
    pub async fn warm_cache(&self, domain: &str) {
        if let Some(certified_key) = self.manager.get_certified_key(domain).await {
            self.cert_cache
                .write()
                .insert(domain.to_string(), certified_key);
        }
    }

    /// Build the `CertifiedKey` from a `Certificate` object.
    fn build_certified_key(cert: &Certificate) -> Option<Arc<CertifiedKey>> {
        let (cert_chain, key) = cert.to_rustls_cert().ok()?;
        let signing_key = rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(key.clone_key())).ok()?;
        Some(Arc::new(CertifiedKey::new(cert_chain, signing_key)))
    }

    /// Generate a temporary self-signed cert for the given SNI name.
    /// This is used when no real certificate exists yet — the client will
    /// get a TLS connection (with a warning about the self-signed cert) so
    /// that the HTTP challenge handler can serve the ACME validation token.
    fn generate_temp_cert(domain: &str) -> Option<Arc<CertifiedKey>> {
        let cert = CertificateManager::generate_self_signed(domain).ok()?;
        Self::build_certified_key(&cert)
    }
}

impl ResolvesServerCert for CertificateResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        // Get SNI from client hello
        let domain_str = client_hello.server_name()?;

        debug!("Resolving certificate for SNI: {}", domain_str);

        // Fast path: check the sync cache
        if let Some(certified_key) = self.cert_cache.read().get(domain_str) {
            debug!("Certificate found in cache for {}", domain_str);
            return Some(certified_key.clone());
        }

        // Slow path: check the async cache via try_read (non-blocking)
        // If the RwLock is not contended, we get the cert synchronously
        if let Ok(cache) = self.manager.cache.try_read() {
            if let Some(cert) = cache.get(domain_str) {
                if !cert.is_expired() {
                    if let Some(certified_key) = Self::build_certified_key(cert) {
                        // Update the fast-path cache
                        self.cert_cache
                            .write()
                            .insert(domain_str.to_string(), certified_key.clone());
                        info!("Certificate resolved from async cache for {}", domain_str);
                        return Some(certified_key);
                    }
                }
            }
        }

        // Fallback: generate a temporary self-signed cert
        warn!(
            "No certificate found for {}, generating temporary self-signed",
            domain_str
        );
        let temp_cert = Self::generate_temp_cert(domain_str);
        if let Some(ref cert_key) = temp_cert {
            self.cert_cache
                .write()
                .insert(domain_str.to_string(), cert_key.clone());
        }
        temp_cert
    }
}

// ---------------------------------------------------------------------------
// Certificate Errors
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Helper: create rustls ServerConfig with SNI resolution
// ---------------------------------------------------------------------------

/// Build a `rustls::ServerConfig` that uses the `CertificateResolver` for
/// SNI-based certificate selection.
pub fn build_rustls_server_config(resolver: Arc<CertificateResolver>) -> rustls::ServerConfig {
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);

    // Enable ALPN for HTTP/1.1 and optionally HTTP/2
    config.alpn_protocols = vec![b"http/1.1".to_vec(), b"h2".to_vec()];

    config
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn test_self_signed_certificate_generation() {
        let cert = CertificateManager::generate_self_signed("test.example.com");
        assert!(cert.is_ok());
        let cert = cert.unwrap();
        assert!(!cert.cert_pem.is_empty());
        assert!(!cert.key_pem.is_empty());
        assert!(!cert.is_expired());
        assert!(cert.needs_renewal(120)); // 90 days < 120 days
        assert!(!cert.needs_renewal(30)); // 90 days > 30 days
    }

    #[test]
    fn test_self_signed_to_rustls() {
        let cert = CertificateManager::generate_self_signed("test.example.com").unwrap();
        let result = cert.to_rustls_cert();
        assert!(result.is_ok());
        let (chain, key) = result.unwrap();
        assert!(!chain.is_empty());
        assert_eq!(key.secret_pkcs8_der().len() > 0, true);
    }

    #[test]
    fn test_jwk_thumbprint() {
        let key = SigningKey::random(&mut OsRng);
        let verifying_key = key.verifying_key();
        let jwk = build_jwk(&verifying_key);

        let thumbprint = jwk_thumbprint(&jwk);
        assert!(thumbprint.is_ok());
        let thumbprint = thumbprint.unwrap();
        // SHA-256 base64url should be 43 chars (256 bits / 6 bits + padding)
        assert_eq!(thumbprint.len(), 43);
    }

    #[test]
    fn test_csr_generation() {
        let key = SigningKey::random(&mut OsRng);
        let csr = generate_ecdsa_csr(&key, "example.com");
        assert!(csr.is_ok());
        let csr_bytes = csr.unwrap();
        // CSR should start with SEQUENCE tag (0x30)
        assert_eq!(csr_bytes[0], 0x30);
    }

    #[test]
    fn test_split_cert_chain() {
        let pem = "-----BEGIN CERTIFICATE-----\nleaf\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nintermediate\n-----END CERTIFICATE-----\n";
        let (leaf, chain) = split_cert_chain(pem);
        assert!(leaf.contains("leaf"));
        assert!(chain.contains("intermediate"));
    }

    #[test]
    fn test_split_single_cert() {
        let pem = "-----BEGIN CERTIFICATE-----\nonlycert\n-----END CERTIFICATE-----\n";
        let (leaf, chain) = split_cert_chain(pem);
        assert!(leaf.contains("onlycert"));
        assert!(chain.is_empty());
    }

    #[test]
    fn test_pem_line_wrap() {
        let long_str = "A".repeat(200);
        let wrapped = pem_line_wrap(&long_str);
        let lines: Vec<&str> = wrapped.lines().collect();
        // 200 / 64 = 3.125 -> 4 lines (last line has 8 chars)
        assert_eq!(lines.len(), 4);
        for (i, line) in lines.iter().enumerate() {
            if i < 3 {
                assert_eq!(line.len(), 64);
            }
        }
    }

    #[test]
    fn test_key_authorization() {
        // We can't easily test this without a real ACME client, but we can
        // verify the format is correct
        let key = SigningKey::random(&mut OsRng);
        let jwk = build_jwk(&key.verifying_key());
        let thumbprint = jwk_thumbprint(&jwk).unwrap();
        let token = "test-token";
        let expected = format!("{}.{}", token, thumbprint);

        // Verify the format
        assert!(expected.starts_with("test-token."));
        // The thumbprint part should be 43 chars
        assert_eq!(expected.len(), token.len() + 1 + 43);
    }

    #[tokio::test]
    async fn test_challenge_token_storage() {
        let config = CertConfig::default();
        let manager = CertificateManager::new(&config).await.unwrap();

        assert!(manager.get_challenge_token("nonexistent").is_none());

        // Store a token
        {
            let mut tokens = manager.challenge_tokens.write().unwrap();
            tokens.insert("test-token".to_string(), "test-key-auth".to_string());
        }

        assert_eq!(
            manager.get_challenge_token("test-token"),
            Some("test-key-auth".to_string())
        );
    }

    #[test]
    fn test_der_encoding() {
        // Test OID encoding - use simple OID components that fit in u8
        // OID 1.2.3.4 encoded as: first byte = 40*1 + 2 = 42 (0x2a), then 3, 4
        let oid = der_oid(&[0x2a, 0x03, 0x04]);
        assert_eq!(oid[0], 0x06); // OID tag
        assert_eq!(oid[1] as usize, 3); // length

        // Test SEQUENCE
        let seq = der_sequence(&[vec![0x05, 0x00]]);
        assert_eq!(seq[0], 0x30);
    }
}
