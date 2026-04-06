//! JWT token generation and validation
//!
//! Uses jsonwebtoken crate with RS256 algorithm (asymmetric) for token signing
//! when RSA keys are configured, falling back to HS256 (symmetric) for legacy
//! compatibility. Supports access tokens (15min default) and refresh tokens (7d default).

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

use crate::auth::{AuthError, UserRole};
use crate::config::JwtConfig;

/// JWT claims for access tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// Subject (user ID)
    pub sub: Uuid,
    /// Username
    pub username: String,
    /// Issued at
    pub iat: i64,
    /// Expiration time
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// JWT ID (unique identifier for revocation)
    pub jti: Option<String>,
    /// User role
    pub role: String,
    /// User permissions
    pub permissions: Vec<String>,
    /// Organization ID
    pub org_id: Option<Uuid>,
    /// Token type
    pub token_type: String,
}

/// JWT claims for refresh tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    /// Subject (user ID)
    pub sub: Uuid,
    /// Username
    pub username: String,
    /// Issued at
    pub iat: i64,
    /// Expiration time
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// JWT ID
    pub jti: Option<String>,
    /// Token type
    pub token_type: String,
}

/// Create an access token
pub fn create_access_token(
    config: &JwtConfig,
    user_id: Uuid,
    username: &str,
    role: &UserRole,
    permissions: &[String],
    org_id: Option<Uuid>,
) -> Result<String, AuthError> {
    let now = Utc::now();
    let exp = now.timestamp() + config.expiry_secs as i64;

    let claims = AccessClaims {
        sub: user_id,
        username: username.to_string(),
        iat: now.timestamp(),
        exp,
        iss: config.issuer.clone(),
        aud: "shellwego-api".to_string(),
        jti: Some(Uuid::new_v4().to_string()),
        role: role.to_string(),
        permissions: permissions.to_vec(),
        org_id,
        token_type: "access".to_string(),
    };

    let key = if let Some(ref private_pem) = config.private_key_pem {
        EncodingKey::from_rsa_pem(private_pem.as_bytes())
            .map_err(|e| AuthError::InternalError(format!("Failed to parse RSA private key: {}", e)))?
    } else {
        tracing::warn!("No RSA private key configured, falling back to HS256 (insecure)");
        EncodingKey::from_secret(config.secret.as_bytes())
    };
    let header = if config.private_key_pem.is_some() {
        Header::new(jsonwebtoken::Algorithm::RS256)
    } else {
        Header::default()
    };

    encode(&header, &claims, &key)
        .map_err(|e| AuthError::InternalError(format!("Failed to create access token: {}", e)))
}

/// Create a refresh token
pub fn create_refresh_token(
    config: &JwtConfig,
    user_id: Uuid,
    username: &str,
) -> Result<String, AuthError> {
    let now = Utc::now();
    let exp = now.timestamp() + config.refresh_expiry_secs as i64;

    let claims = RefreshClaims {
        sub: user_id,
        username: username.to_string(),
        iat: now.timestamp(),
        exp,
        iss: config.issuer.clone(),
        aud: "shellwego-api".to_string(),
        jti: Some(Uuid::new_v4().to_string()),
        token_type: "refresh".to_string(),
    };

    let key = if let Some(ref private_pem) = config.private_key_pem {
        EncodingKey::from_rsa_pem(private_pem.as_bytes())
            .map_err(|e| AuthError::InternalError(format!("Failed to parse RSA private key: {}", e)))?
    } else {
        tracing::warn!("No RSA private key configured, falling back to HS256 (insecure)");
        EncodingKey::from_secret(config.secret.as_bytes())
    };
    let header = if config.private_key_pem.is_some() {
        Header::new(jsonwebtoken::Algorithm::RS256)
    } else {
        Header::default()
    };

    encode(&header, &claims, &key)
        .map_err(|e| AuthError::InternalError(format!("Failed to create refresh token: {}", e)))
}

/// Validate a token and return its claims
///
/// When `allow_refresh` is true, refresh tokens are also accepted.
pub fn validate_token(
    config: &JwtConfig,
    token_str: &str,
    allow_refresh: bool,
) -> Result<AccessClaims, AuthError> {
    let (key, alg) = if let Some(ref public_pem) = config.public_key_pem {
        let k = DecodingKey::from_rsa_pem(public_pem.as_bytes())
            .map_err(|e| AuthError::InvalidToken(format!("Failed to parse RSA public key: {}", e)))?;
        (k, jsonwebtoken::Algorithm::RS256)
    } else {
        (DecodingKey::from_secret(config.secret.as_bytes()), jsonwebtoken::Algorithm::HS256)
    };
    let mut validation = Validation::new(alg);
    validation.leeway = 0;
    validation.set_issuer(&[&config.issuer]);
    validation.set_audience(&["shellwego-api"]);

    let token_data = decode::<AccessClaims>(token_str, &key, &validation);

    match token_data {
        Ok(data) => {
            let claims = data.claims;

            if claims.token_type == "refresh" && !allow_refresh {
                return Err(AuthError::InvalidToken(
                    "Refresh token cannot be used as access token".to_string(),
                ));
            }

            Ok(claims)
        }
        Err(e) => {
            // If access token decode failed but allow_refresh is true,
            // try decoding as RefreshClaims (which has fewer fields)
            if allow_refresh {
                match decode::<RefreshClaims>(token_str, &key, &validation) {
                    Ok(data) => {
                        let rc = data.claims;
                        Ok(AccessClaims {
                            sub: rc.sub,
                            username: rc.username,
                            iat: rc.iat,
                            exp: rc.exp,
                            iss: rc.iss,
                            aud: rc.aud,
                            jti: rc.jti,
                            role: String::new(),
                            permissions: Vec::new(),
                            org_id: None,
                            token_type: rc.token_type,
                        })
                    }
                    Err(refresh_err) => match refresh_err.kind() {
                        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                            Err(AuthError::TokenExpired)
                        }
                        _ => Err(AuthError::InvalidToken(format!(
                            "Invalid token: {}",
                            e
                        ))),
                    },
                }
            } else {
                match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => Err(AuthError::TokenExpired),
                    _ => Err(AuthError::InvalidToken(format!(
                        "Invalid token: {}",
                        e
                    ))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        let private_key = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
        let public_key = private_key.to_public_key();
        let private_pem = private_key.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let public_pem = public_key.to_public_key_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        JwtConfig {
            secret: String::new(),
            private_key_pem: Some(private_pem),
            public_key_pem: Some(public_pem),
            issuer: "shellwego-test".to_string(),
            expiry_secs: 900,
            refresh_expiry_secs: 604800,
        }
    }

    #[test]
    fn test_create_access_token_is_jwt() {
        let config = test_config();
        let token = create_access_token(
            &config,
            Uuid::new_v4(),
            "testuser",
            &UserRole::Admin,
            &["apps:read".to_string()],
            None,
        );
        assert!(token.is_ok());
        let token_str = token.unwrap();
        let parts: Vec<&str> = token_str.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have 3 dot-separated parts");
    }

    #[test]
    fn test_create_refresh_token_is_jwt() {
        let config = test_config();
        let token = create_refresh_token(&config, Uuid::new_v4(), "testuser");
        assert!(token.is_ok());
    }

    #[test]
    fn test_validate_access_token_claims() {
        let config = test_config();
        let user_id = Uuid::new_v4();
        let token = create_access_token(
            &config,
            user_id,
            "testuser",
            &UserRole::Admin,
            &["apps:read".to_string(), "apps:write".to_string()],
            None,
        )
        .unwrap();

        let claims = validate_token(&config, &token, false).unwrap();
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.username, "testuser");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.token_type, "access");
        assert_eq!(claims.permissions.len(), 2);
    }

    #[test]
    fn test_validate_refresh_token_allowed() {
        let config = test_config();
        let user_id = Uuid::new_v4();
        let token = create_refresh_token(&config, user_id, "testuser").unwrap();

        let claims = validate_token(&config, &token, true).unwrap();
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.token_type, "refresh");
    }

    #[test]
    fn test_refresh_token_rejected_as_access() {
        let config = test_config();
        let user_id = Uuid::new_v4();
        let token = create_refresh_token(&config, user_id, "testuser").unwrap();

        let result = validate_token(&config, &token, false);
        assert!(result.is_err());
        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_invalid_token_rejected() {
        let config = test_config();
        let result = validate_token(&config, "not.a.valid.token", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let config = test_config();
        let user_id = Uuid::new_v4();
        let token = create_access_token(
            &config,
            user_id,
            "testuser",
            &UserRole::Member,
            &[],
            None,
        )
        .unwrap();

        // Generate a different RSA keypair for the "wrong" config
        let wrong_private_key = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
        let wrong_public_key = wrong_private_key.to_public_key();
        let wrong_private_pem = wrong_private_key.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let wrong_public_pem = wrong_public_key.to_public_key_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let wrong_config = JwtConfig {
            secret: "wrong-secret-key-for-testing".to_string(),
            private_key_pem: Some(wrong_private_pem),
            public_key_pem: Some(wrong_public_pem),
            issuer: "shellwego-test".to_string(),
            expiry_secs: 900,
            refresh_expiry_secs: 604800,
        };
        let result = validate_token(&wrong_config, &token, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_expired_token_rejected() {
        let private_key = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
        let public_key = private_key.to_public_key();
        let private_pem = private_key.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let public_pem = public_key.to_public_key_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let config = JwtConfig {
            secret: String::new(),
            private_key_pem: Some(private_pem),
            public_key_pem: Some(public_pem),
            issuer: "shellwego-test".to_string(),
            expiry_secs: 0,
            refresh_expiry_secs: 0,
        };

        let token = create_access_token(
            &config,
            Uuid::new_v4(),
            "testuser",
            &UserRole::Member,
            &[],
            None,
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let result = validate_token(&config, &token, false);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }
}
