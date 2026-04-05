//! Authentication and authorization module
//!
//! Provides JWT-based authentication, password hashing with argon2id,
//! role-based access control (RBAC), and token revocation.

pub mod jwt;
pub mod password;
pub mod rbac;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::config::JwtConfig;

/// User record stored in memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub email: String,
    pub organization_id: Option<Uuid>,
    pub role: UserRole,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserRole {
    Admin,
    Member,
    ReadOnly,
}

impl Default for UserRole {
    fn default() -> Self {
        UserRole::Member
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Admin => write!(f, "admin"),
            UserRole::Member => write!(f, "member"),
            UserRole::ReadOnly => write!(f, "read_only"),
        }
    }
}

/// Currently authenticated user (extracted from JWT)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: UserRole,
    pub permissions: Vec<String>,
    pub organization_id: Option<Uuid>,
}

/// Authentication service
pub struct AuthService {
    /// User storage: user_id -> UserRecord
    users: Arc<DashMap<Uuid, UserRecord>>,
    /// Username to user_id index
    usernames: Arc<DashMap<String, Uuid>>,
    /// Token blocklist for logout/revocation: jti -> expiry_time
    token_blocklist: Arc<DashMap<String, Instant>>,
    /// JWT configuration
    jwt_config: JwtConfig,
}

/// Result of authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub user: UserRecord,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

impl AuthService {
    /// Create a new auth service
    pub fn new(jwt_config: JwtConfig) -> Self {
        info!("Initializing authentication service");
        Self {
            users: Arc::new(DashMap::new()),
            usernames: Arc::new(DashMap::new()),
            token_blocklist: Arc::new(DashMap::new()),
            jwt_config,
        }
    }

    /// Register a new user
    pub async fn register(
        &self,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<AuthResult, AuthError> {
        // Check if username already exists
        if self.usernames.contains_key(username) {
            return Err(AuthError::UserAlreadyExists(username.to_string()));
        }

        // Hash the password with argon2id
        let password_hash = password::hash_password(password)
            .map_err(|e| AuthError::InternalError(e.to_string()))?;

        let user_id = Uuid::new_v4();
        let now = Utc::now();

        let role = UserRole::Member;
        let permissions = role.default_permissions();

        let user = UserRecord {
            id: user_id,
            username: username.to_string(),
            password_hash,
            email: email.to_string(),
            organization_id: None,
            role,
            permissions: permissions.clone(),
            created_at: now,
            updated_at: now,
        };

        self.users.insert(user_id, user.clone());
        self.usernames.insert(username.to_string(), user_id);

        info!("Registered new user: {} (id: {})", username, user_id);

        let access_token = jwt::create_access_token(
            &self.jwt_config,
            user_id,
            username,
            &user.role,
            &permissions,
            user.organization_id,
        )?;

        let refresh_token = jwt::create_refresh_token(&self.jwt_config, user_id, username)?;

        Ok(AuthResult {
            user,
            access_token,
            refresh_token,
            expires_in: self.jwt_config.expiry_secs,
        })
    }

    /// Authenticate a user (login)
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthResult, AuthError> {
        let user_id = self
            .usernames
            .get(username)
            .ok_or(AuthError::InvalidCredentials)?
            .value()
            .clone();

        let user = self
            .users
            .get(&user_id)
            .ok_or(AuthError::InvalidCredentials)?
            .value()
            .clone();

        let valid = password::verify_password(password, &user.password_hash)
            .map_err(|e| AuthError::InternalError(e.to_string()))?;

        if !valid {
            return Err(AuthError::InvalidCredentials);
        }

        info!("User logged in: {} (id: {})", username, user_id);

        let access_token = jwt::create_access_token(
            &self.jwt_config,
            user.id,
            &user.username,
            &user.role,
            &user.permissions,
            user.organization_id,
        )?;

        let refresh_token =
            jwt::create_refresh_token(&self.jwt_config, user.id, &user.username)?;

        Ok(AuthResult {
            user,
            access_token,
            refresh_token,
            expires_in: self.jwt_config.expiry_secs,
        })
    }

    /// Refresh an access token using a refresh token
    pub async fn refresh_token(&self, refresh_token_str: &str) -> Result<AuthResult, AuthError> {
        let claims = jwt::validate_token(&self.jwt_config, refresh_token_str, true)?;

        // Check if token is blocklisted
        if let Some(jti) = &claims.jti {
            if self.token_blocklist.contains_key(jti) {
                return Err(AuthError::TokenRevoked);
            }
        }

        let user = self
            .users
            .get(&claims.sub)
            .ok_or(AuthError::UserNotFound)?
            .value()
            .clone();

        let access_token = jwt::create_access_token(
            &self.jwt_config,
            user.id,
            &user.username,
            &user.role,
            &user.permissions,
            user.organization_id,
        )?;

        let new_refresh_token =
            jwt::create_refresh_token(&self.jwt_config, user.id, &user.username)?;

        Ok(AuthResult {
            user,
            access_token,
            refresh_token: new_refresh_token,
            expires_in: self.jwt_config.expiry_secs,
        })
    }

    /// Revoke a token (logout)
    pub async fn revoke_token(&self, token_str: &str) -> Result<(), AuthError> {
        let claims = jwt::validate_token(&self.jwt_config, token_str, false)?;

        if let Some(jti) = claims.jti {
            let now_ts = Utc::now().timestamp();
            let remaining = if claims.exp > now_ts {
                (claims.exp - now_ts) as u64
            } else {
                0
            };
            let expiry = Instant::now() + std::time::Duration::from_secs(remaining);
            self.token_blocklist.insert(jti, expiry);
            info!("Revoked token: {}", jti);
        }

        Ok(())
    }

    /// Validate an access token and return the current user
    pub async fn validate_access_token(&self, token_str: &str) -> Result<CurrentUser, AuthError> {
        let claims = jwt::validate_token(&self.jwt_config, token_str, false)?;

        // Check if token is blocklisted
        if let Some(jti) = &claims.jti {
            if self.token_blocklist.contains_key(jti) {
                return Err(AuthError::TokenRevoked);
            }
        }

        let user = self
            .users
            .get(&claims.sub)
            .ok_or(AuthError::UserNotFound)?;

        Ok(CurrentUser {
            user_id: user.id,
            username: user.username.clone(),
            role: user.role,
            permissions: user.permissions.clone(),
            organization_id: user.organization_id,
        })
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: &Uuid) -> Option<UserRecord> {
        self.users.get(user_id).map(|r| r.value().clone())
    }

    /// Clean up expired blocklisted tokens
    pub async fn cleanup_blocklist(&self) {
        let now = Instant::now();
        self.token_blocklist.retain(|_, expiry| *expiry > now);
    }
}

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User already exists: {0}")]
    UserAlreadyExists(String),

    #[error("User not found")]
    UserNotFound,

    #[error("Token has expired")]
    TokenExpired,

    #[error("Token is invalid: {0}")]
    InvalidToken(String),

    #[error("Token has been revoked")]
    TokenRevoked,

    #[error("Insufficient permissions: required {required}, have {have}")]
    InsufficientPermissions {
        required: String,
        have: String,
    },

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl UserRole {
    /// Get default permissions for this role
    pub fn default_permissions(&self) -> Vec<String> {
        match self {
            UserRole::Admin => vec![
                "admin:*".to_string(),
                "apps:read".to_string(),
                "apps:write".to_string(),
                "apps:delete".to_string(),
                "nodes:read".to_string(),
                "nodes:write".to_string(),
                "nodes:delete".to_string(),
                "volumes:read".to_string(),
                "volumes:write".to_string(),
                "volumes:delete".to_string(),
                "domains:read".to_string(),
                "domains:write".to_string(),
                "domains:delete".to_string(),
                "databases:read".to_string(),
                "databases:write".to_string(),
                "databases:delete".to_string(),
                "secrets:read".to_string(),
                "secrets:write".to_string(),
                "secrets:delete".to_string(),
                "organizations:read".to_string(),
                "organizations:write".to_string(),
                "builds:read".to_string(),
                "builds:write".to_string(),
                "users:read".to_string(),
                "users:write".to_string(),
                "audit:read".to_string(),
            ],
            UserRole::Member => vec![
                "apps:read".to_string(),
                "apps:write".to_string(),
                "nodes:read".to_string(),
                "volumes:read".to_string(),
                "volumes:write".to_string(),
                "domains:read".to_string(),
                "domains:write".to_string(),
                "databases:read".to_string(),
                "databases:write".to_string(),
                "secrets:read".to_string(),
                "secrets:write".to_string(),
                "organizations:read".to_string(),
                "builds:read".to_string(),
                "builds:write".to_string(),
            ],
            UserRole::ReadOnly => vec![
                "apps:read".to_string(),
                "nodes:read".to_string(),
                "volumes:read".to_string(),
                "domains:read".to_string(),
                "databases:read".to_string(),
                "secrets:read".to_string(),
                "organizations:read".to_string(),
                "builds:read".to_string(),
            ],
        }
    }

    /// Check if a role has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        let perms = self.default_permissions();
        for p in &perms {
            if matches_permission(p, permission) {
                return true;
            }
        }
        false
    }
}

/// Check if a granted permission matches a required permission
pub fn matches_permission(granted: &str, required: &str) -> bool {
    // admin:* grants everything
    if granted == "admin:*" {
        return true;
    }
    // Exact match
    if granted == required {
        return true;
    }
    // Wildcard match: e.g., "apps:*" matches "apps:read"
    let g_parts: Vec<&str> = granted.split(':').collect();
    let r_parts: Vec<&str> = required.split(':').collect();
    if g_parts.len() >= 2 && g_parts[1] == "*" && g_parts[0] == r_parts[0] {
        return true;
    }
    false
}

/// Check if a list of permissions includes a required permission
pub fn has_permission(permissions: &[String], required: &str) -> bool {
    for p in permissions {
        if matches_permission(p, required) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key-for-testing-that-is-long-enough".to_string(),
            issuer: "shellwego-test".to_string(),
            expiry_secs: 900,
            refresh_expiry_secs: 604800,
        }
    }

    #[tokio::test]
    async fn test_register_user() {
        let auth = AuthService::new(test_jwt_config());
        let result = auth
            .register("testuser", "test@example.com", "password123")
            .await;
        assert!(result.is_ok());
        let auth_result = result.unwrap();
        assert_eq!(auth_result.user.username, "testuser");
        assert!(!auth_result.access_token.is_empty());
        assert!(!auth_result.refresh_token.is_empty());
        // Verify it's a real JWT (3 dot-separated parts)
        let parts: Vec<&str> = auth_result.access_token.split('.').collect();
        assert_eq!(parts.len(), 3);
    }

    #[tokio::test]
    async fn test_register_duplicate_user() {
        let auth = AuthService::new(test_jwt_config());
        auth.register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        let result = auth
            .register("testuser", "test2@example.com", "password456")
            .await;
        assert!(matches!(result, Err(AuthError::UserAlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_login_success() {
        let auth = AuthService::new(test_jwt_config());
        auth.register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        let result = auth.login("testuser", "password123").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let auth = AuthService::new(test_jwt_config());
        auth.register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        let result = auth.login("testuser", "wrongpassword").await;
        assert!(matches!(result, Err(AuthError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        let auth = AuthService::new(test_jwt_config());
        let result = auth.login("nonexistent", "password").await;
        assert!(matches!(result, Err(AuthError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_validate_access_token() {
        let auth = AuthService::new(test_jwt_config());
        let result = auth
            .register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        let user = auth.validate_access_token(&result.access_token).await;
        assert!(user.is_ok());
        let current_user = user.unwrap();
        assert_eq!(current_user.username, "testuser");
    }

    #[tokio::test]
    async fn test_refresh_token_flow() {
        let auth = AuthService::new(test_jwt_config());
        let result = auth
            .register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        let refreshed = auth.refresh_token(&result.refresh_token).await;
        assert!(refreshed.is_ok());
        let refreshed_result = refreshed.unwrap();
        assert!(!refreshed_result.access_token.is_empty());
        assert_ne!(refreshed_result.access_token, result.access_token);
    }

    #[tokio::test]
    async fn test_revoke_token() {
        let auth = AuthService::new(test_jwt_config());
        let result = auth
            .register("testuser", "test@example.com", "password123")
            .await
            .unwrap();
        auth.revoke_token(&result.access_token).await.unwrap();
        let validation = auth.validate_access_token(&result.access_token).await;
        assert!(matches!(validation, Err(AuthError::TokenRevoked)));
    }

    #[test]
    fn test_admin_has_all_permissions() {
        assert!(UserRole::Admin.has_permission("apps:read"));
        assert!(UserRole::Admin.has_permission("apps:write"));
        assert!(UserRole::Admin.has_permission("nodes:read"));
        assert!(UserRole::Admin.has_permission("secrets:write"));
    }

    #[test]
    fn test_readonly_limited_permissions() {
        assert!(UserRole::ReadOnly.has_permission("apps:read"));
        assert!(!UserRole::ReadOnly.has_permission("apps:write"));
        assert!(!UserRole::ReadOnly.has_permission("apps:delete"));
    }

    #[test]
    fn test_member_permissions() {
        assert!(UserRole::Member.has_permission("apps:read"));
        assert!(UserRole::Member.has_permission("apps:write"));
        assert!(!UserRole::Member.has_permission("apps:delete"));
        assert!(!UserRole::Member.has_permission("users:write"));
    }

    #[test]
    fn test_matches_permission_wildcard() {
        assert!(matches_permission("admin:*", "apps:read"));
        assert!(matches_permission("apps:*", "apps:read"));
        assert!(matches_permission("apps:*", "apps:write"));
        assert!(!matches_permission("apps:read", "apps:write"));
        assert!(!matches_permission("apps:read", "nodes:read"));
    }
}
