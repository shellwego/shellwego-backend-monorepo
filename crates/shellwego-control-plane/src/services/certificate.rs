//! Certificate management service
//!
//! Handles TLS certificate lifecycle including ACME/Let's Encrypt integration,
//! certificate storage, and auto-renewal.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc, TimeZone};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Certificate service configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CertificateConfig {
    /// ACME directory URL
    pub acme_directory: String,
    /// ACME account email
    pub acme_email: String,
    /// Enable staging mode (use Let's Encrypt staging)
    pub staging: bool,
    /// Default key type
    pub default_key_type: KeyType,
    /// Auto-renewal days before expiry
    pub renewal_days: i64,
    /// Challenge type preference
    pub challenge_type: ChallengeType,
}

impl Default for CertificateConfig {
    fn default() -> Self {
        Self {
            acme_directory: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            acme_email: String::new(),
            staging: false,
            default_key_type: KeyType::EcdsaP256,
            renewal_days: 30,
            challenge_type: ChallengeType::Http01,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyType {
    Rsa2048,
    Rsa4096,
    EcdsaP256,
    EcdsaP384,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChallengeType {
    Http01,
    Dns01,
    TlsAlpn01,
}

/// Certificate metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateMetadata {
    pub id: Uuid,
    pub domain: String,
    pub san_domains: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: CertificateStatus,
    pub key_type: KeyType,
    pub issuer: String,
    pub serial_number: String,
    pub auto_renew: bool,
    pub last_renewal_attempt: Option<DateTime<Utc>>,
    pub renewal_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CertificateStatus {
    Pending,
    Validating,
    Issued,
    Renewing,
    Expired,
    Revoked,
    Failed { error: String },
}

/// ACME challenge record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeChallenge {
    pub id: Uuid,
    pub domain: String,
    pub token: String,
    pub key_authorization: String,
    pub challenge_type: ChallengeType,
    pub status: ChallengeStatus,
    pub created_at: DateTime<Utc>,
    pub validated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChallengeStatus {
    Pending,
    Processing,
    Valid,
    Invalid { error: String },
}

/// Certificate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    pub domain: String,
    pub san_domains: Vec<String>,
    pub key_type: Option<KeyType>,
    pub auto_renew: bool,
    pub challenge_type: Option<ChallengeType>,
}

/// Certificate service
pub struct CertificateService {
    config: CertificateConfig,
    certificates: Arc<RwLock<HashMap<Uuid, CertificateMetadata>>>,
    challenges: Arc<RwLock<HashMap<String, AcmeChallenge>>>,
}

impl CertificateService {
    /// Create a new certificate service
    pub fn new(config: CertificateConfig) -> Self {
        info!("Initializing certificate service with ACME directory: {}", config.acme_directory);
        
        Self {
            config,
            certificates: Arc::new(RwLock::new(HashMap::new())),
            challenges: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Request a new certificate
    pub async fn request_certificate(
        &self,
        request: CertificateRequest,
    ) -> Result<CertificateMetadata, CertificateError> {
        let cert_id = Uuid::new_v4();
        let key_type = request.key_type.clone().unwrap_or_else(|| self.config.default_key_type.clone());
        let challenge_type = request.challenge_type.clone().unwrap_or_else(|| self.config.challenge_type.clone());

        info!("Requesting certificate for domain: {} (SANs: {:?})", request.domain, request.san_domains);

        // Create pending certificate record
        let mut cert = CertificateMetadata {
            id: cert_id,
            domain: request.domain.clone(),
            san_domains: request.san_domains.clone(),
            created_at: Utc::now(),
            expires_at: Utc::now(), // Will be updated on issuance
            status: CertificateStatus::Pending,
            key_type,
            issuer: "Let's Encrypt".to_string(),
            serial_number: String::new(),
            auto_renew: request.auto_renew,
            last_renewal_attempt: None,
            renewal_error: None,
        };

        // Store initial record
        {
            let mut certs = self.certificates.write().await;
            certs.insert(cert_id, cert.clone());
        }

        // Create ACME challenge
        let challenge = self.create_challenge(&request.domain, challenge_type).await?;

        // Update status to validating
        cert.status = CertificateStatus::Validating;
        self.update_certificate(&cert).await;

        // Simulate ACME validation and issuance
        match self.simulate_acme_issuance(&mut cert, &challenge).await {
            Ok(()) => {
                info!("Certificate {} issued successfully for domain {}", cert_id, cert.domain);
                Ok(cert)
            }
            Err(e) => {
                cert.status = CertificateStatus::Failed { error: e.to_string() };
                self.update_certificate(&cert).await;
                Err(e)
            }
        }
    }

    /// Create ACME challenge
    async fn create_challenge(
        &self,
        domain: &str,
        challenge_type: ChallengeType,
    ) -> Result<AcmeChallenge, CertificateError> {
        let challenge_id = Uuid::new_v4();
        let token = format!("{:x}", Uuid::new_v4());
        let key_authorization = format!("{}.{}", token, self.get_account_key_thumbprint());

        let challenge = AcmeChallenge {
            id: challenge_id,
            domain: domain.to_string(),
            token: token.clone(),
            key_authorization,
            challenge_type,
            status: ChallengeStatus::Pending,
            created_at: Utc::now(),
            validated_at: None,
        };

        {
            let mut challenges = self.challenges.write().await;
            challenges.insert(token.clone(), challenge.clone());
        }

        debug!("Created {} challenge for domain: {}", 
            challenge.challenge_type.as_ref(), domain);
        Ok(challenge)
    }

    /// Get account key thumbprint (simulated)
    fn get_account_key_thumbprint(&self) -> String {
        // In production, this would compute the JWK thumbprint
        format!("thumbprint-{:x}", Uuid::new_v4())
    }

    /// Simulate ACME issuance
    async fn simulate_acme_issuance(
        &self,
        cert: &mut CertificateMetadata,
        challenge: &AcmeChallenge,
    ) -> Result<(), CertificateError> {
        // Simulate challenge validation
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Update challenge status
        {
            let mut challenges = self.challenges.write().await;
            if let Some(c) = challenges.get_mut(&challenge.token) {
                c.status = ChallengeStatus::Valid;
                c.validated_at = Some(Utc::now());
            }
        }

        // Simulate certificate issuance
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Set expiry (90 days from now for Let's Encrypt)
        cert.expires_at = Utc::now() + chrono::Duration::days(90);
        cert.status = CertificateStatus::Issued;
        cert.serial_number = format!("{:032x}", Uuid::new_v4());

        self.update_certificate(cert).await;
        Ok(())
    }

    /// Update certificate in storage
    async fn update_certificate(&self, cert: &CertificateMetadata) {
        let mut certs = self.certificates.write().await;
        certs.insert(cert.id, cert.clone());
    }

    /// Get certificate by domain
    pub async fn get_certificate_by_domain(&self, domain: &str) -> Option<CertificateMetadata> {
        let certs = self.certificates.read().await;
        certs.values()
            .find(|c| c.domain == domain || c.san_domains.contains(&domain.to_string()))
            .cloned()
    }

    /// Get certificate by ID
    pub async fn get_certificate(&self, cert_id: &Uuid) -> Option<CertificateMetadata> {
        let certs = self.certificates.read().await;
        certs.get(cert_id).cloned()
    }

    /// List all certificates
    pub async fn list_certificates(&self) -> Vec<CertificateMetadata> {
        let certs = self.certificates.read().await;
        certs.values().cloned().collect()
    }

    /// Renew a certificate
    pub async fn renew_certificate(&self, cert_id: &Uuid) -> Result<CertificateMetadata, CertificateError> {
        let mut cert = {
            let certs = self.certificates.read().await;
            certs.get(cert_id).cloned()
                .ok_or_else(|| CertificateError::NotFound(*cert_id))?
        };

        if cert.status != CertificateStatus::Issued && cert.status != CertificateStatus::Expired {
            return Err(CertificateError::InvalidState(
                format!("Certificate is in {:?} state", cert.status)
            ));
        }

        info!("Renewing certificate {} for domain {}", cert_id, cert.domain);
        cert.status = CertificateStatus::Renewing;
        cert.last_renewal_attempt = Some(Utc::now());
        self.update_certificate(&cert).await;

        // Create new challenge
        let challenge = self.create_challenge(&cert.domain, self.config.challenge_type.clone()).await?;

        // Simulate renewal
        match self.simulate_acme_issuance(&mut cert, &challenge).await {
            Ok(()) => {
                cert.renewal_error = None;
                info!("Certificate {} renewed successfully", cert_id);
                Ok(cert)
            }
            Err(e) => {
                cert.renewal_error = Some(e.to_string());
                cert.status = CertificateStatus::Failed { error: e.to_string() };
                self.update_certificate(&cert).await;
                Err(e)
            }
        }
    }

    /// Revoke a certificate
    pub async fn revoke_certificate(&self, cert_id: &Uuid, reason: Option<&str>) -> Result<(), CertificateError> {
        let mut cert = {
            let certs = self.certificates.read().await;
            certs.get(cert_id).cloned()
                .ok_or_else(|| CertificateError::NotFound(*cert_id))?
        };

        info!("Revoking certificate {} for domain {} (reason: {:?})", 
            cert_id, cert.domain, reason);

        // Simulate ACME revocation
        tokio::time::sleep(Duration::from_millis(50)).await;

        cert.status = CertificateStatus::Revoked;
        self.update_certificate(&cert).await;

        Ok(())
    }

    /// Check and auto-renew certificates
    pub async fn check_renewals(&self) -> Result<Vec<Uuid>, CertificateError> {
        info!("Checking certificates for renewal");
        let mut renewed = Vec::new();

        let certs = self.certificates.read().await;
        for cert in certs.values() {
            if !cert.auto_renew {
                continue;
            }

            let days_until_expiry = (cert.expires_at - Utc::now()).num_days();
            if days_until_expiry <= self.config.renewal_days {
                debug!("Certificate {} for {} expires in {} days, renewing",
                    cert.id, cert.domain, days_until_expiry);
                
                match self.renew_certificate(&cert.id).await {
                    Ok(renewed_cert) => renewed.push(renewed_cert.id),
                    Err(e) => {
                        error!("Failed to renew certificate {}: {}", cert.id, e);
                    }
                }
            }
        }

        info!("Auto-renewal check complete: {} certificates renewed", renewed.len());
        Ok(renewed)
    }

    /// Get ACME challenge for HTTP-01 validation
    pub async fn get_http_challenge(&self, token: &str) -> Option<AcmeChallenge> {
        let challenges = self.challenges.read().await;
        challenges.get(token).cloned()
    }

    /// Get DNS challenge record
    pub async fn get_dns_challenge(&self, domain: &str) -> Option<(String, String)> {
        let challenges = self.challenges.read().await;
        challenges.values()
            .find(|c| c.domain == domain && c.challenge_type == ChallengeType::Dns01)
            .map(|c| {
                let record_name = format!("_acme-challenge.{}", c.domain);
                (record_name, c.key_authorization.clone())
            })
    }
}

impl ChallengeType {
    pub fn as_ref(&self) -> &'static str {
        match self {
            ChallengeType::Http01 => "http-01",
            ChallengeType::Dns01 => "dns-01",
            ChallengeType::TlsAlpn01 => "tls-alpn-01",
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CertificateError {
    #[error("Certificate not found: {0}")]
    NotFound(Uuid),
    
    #[error("Certificate request failed: {0}")]
    RequestFailed(String),
    
    #[error("Challenge failed: {0}")]
    ChallengeFailed(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("ACME error: {0}")]
    AcmeError(String),
    
    #[error("Domain validation failed: {0}")]
    ValidationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_certificate() {
        let service = CertificateService::new(CertificateConfig::default());
        
        let request = CertificateRequest {
            domain: "example.com".to_string(),
            san_domains: vec!["www.example.com".to_string()],
            key_type: None,
            auto_renew: true,
            challenge_type: None,
        };
        
        let cert = service.request_certificate(request).await.unwrap();
        assert_eq!(cert.domain, "example.com");
        assert_eq!(cert.status, CertificateStatus::Issued);
    }

    #[tokio::test]
    async fn test_renew_certificate() {
        let service = CertificateService::new(CertificateConfig::default());
        
        let request = CertificateRequest {
            domain: "test.example.com".to_string(),
            san_domains: vec![],
            key_type: None,
            auto_renew: true,
            challenge_type: None,
        };
        
        let cert = service.request_certificate(request).await.unwrap();
        let renewed = service.renew_certificate(&cert.id).await.unwrap();
        
        assert_eq!(renewed.status, CertificateStatus::Issued);
    }

    #[tokio::test]
    async fn test_revoke_certificate() {
        let service = CertificateService::new(CertificateConfig::default());
        
        let request = CertificateRequest {
            domain: "revoke.example.com".to_string(),
            san_domains: vec![],
            key_type: None,
            auto_renew: false,
            challenge_type: None,
        };
        
        let cert = service.request_certificate(request).await.unwrap();
        service.revoke_certificate(&cert.id, Some("superseded")).await.unwrap();
        
        let revoked = service.get_certificate(&cert.id).await.unwrap();
        assert_eq!(revoked.status, CertificateStatus::Revoked);
    }
}
