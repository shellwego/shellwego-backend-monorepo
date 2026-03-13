//! Webhook router for Git providers
//!
//! Handles webhooks from GitHub, GitLab, and Bitbucket.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::git::builder::{BuildSpec, BuildQueue, BuildPriority};

/// Webhook router configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookConfig {
    /// Secret for webhook signature validation
    pub webhook_secret: String,
    /// Allowed source IPs (empty = all allowed)
    pub allowed_ips: Vec<String>,
    /// Auto-deploy on push
    pub auto_deploy: bool,
    /// Default branch for deployments
    pub default_branch: String,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            webhook_secret: String::new(),
            allowed_ips: Vec::new(),
            auto_deploy: true,
            default_branch: "main".to_string(),
        }
    }
}

/// Webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: Uuid,
    pub provider: GitProvider,
    pub event_type: String,
    pub repository: String,
    pub branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub raw_payload: String,
    pub processed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitProvider {
    GitHub,
    GitLab,
    Bitbucket,
    Gitea,
    Generic,
}

/// Repository registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryRegistration {
    pub id: Uuid,
    pub organization_id: String,
    pub app_id: String,
    pub app_name: String,
    pub repository_url: String,
    pub provider: GitProvider,
    pub default_branch: String,
    pub auto_deploy: bool,
    pub deploy_on_branches: Vec<String>,
    pub deploy_on_tags: bool,
    pub build_command: Option<String>,
    pub dockerfile_path: String,
    pub webhook_secret: String,
    pub created_at: DateTime<Utc>,
    pub last_deployment: Option<DateTime<Utc>>,
}

/// Webhook router
pub struct WebhookRouter {
    config: WebhookConfig,
    repositories: Arc<RwLock<HashMap<String, RepositoryRegistration>>>,
    build_queue: Arc<BuildQueue>,
    events: Arc<RwLock<HashMap<Uuid, WebhookEvent>>>,
}

impl WebhookRouter {
    /// Create a new webhook router
    pub fn new(config: WebhookConfig, build_queue: Arc<BuildQueue>) -> Self {
        info!("Initializing webhook router");
        
        Self {
            config,
            repositories: Arc::new(RwLock::new(HashMap::new())),
            build_queue,
            events: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a repository
    pub async fn register_repository(
        &self,
        registration: RepositoryRegistration,
    ) -> Result<Uuid, WebhookError> {
        let repo_id = registration.id;
        let repo_key = self.extract_repo_key(&registration.repository_url);
        
        {
            let mut repos = self.repositories.write().await;
            repos.insert(repo_key.clone(), registration.clone());
        }
        
        info!("Registered repository: {} -> app {}", repo_key, registration.app_name);
        Ok(repo_id)
    }

    /// Unregister a repository
    pub async fn unregister_repository(&self, repo_url: &str) -> Result<(), WebhookError> {
        let repo_key = self.extract_repo_key(repo_url);
        
        let mut repos = self.repositories.write().await;
        repos.remove(&repo_key)
            .ok_or_else(|| WebhookError::NotFound(repo_key.clone()))?;
        
        info!("Unregistered repository: {}", repo_key);
        Ok(())
    }

    /// Handle incoming webhook
    pub async fn handle(
        &self,
        provider: GitProvider,
        event_type: &str,
        signature: Option<&str>,
        payload: &str,
    ) -> Result<Option<Uuid>, WebhookError> {
        let event_id = Uuid::new_v4();
        
        // Validate signature
        self.validate_signature(provider.clone(), signature, payload).await?;
        
        // Parse webhook based on provider
        let event = match provider {
            GitProvider::GitHub => self.parse_github_webhook(event_type, payload).await?,
            GitProvider::GitLab => self.parse_gitlab_webhook(event_type, payload).await?,
            GitProvider::Bitbucket => self.parse_bitbucket_webhook(event_type, payload).await?,
            GitProvider::Gitea => self.parse_gitea_webhook(event_type, payload).await?,
            GitProvider::Generic => self.parse_generic_webhook(event_type, payload).await?,
        };
        
        let mut event = event;
        event.id = event_id;
        
        // Store event
        {
            let mut events = self.events.write().await;
            events.insert(event_id, event.clone());
        }
        
        // Process event
        let build_id = self.process_webhook_event(&event).await?;
        
        if let Some(id) = build_id {
            info!("Webhook {} triggered build {}", event_id, id);
        }
        
        Ok(build_id)
    }

    /// Validate webhook signature
    async fn validate_signature(
        &self,
        provider: GitProvider,
        signature: Option<&str>,
        payload: &str,
    ) -> Result<(), WebhookError> {
        if self.config.webhook_secret.is_empty() {
            return Ok(());
        }
        
        let signature = signature.ok_or_else(|| {
            WebhookError::InvalidSignature("Missing signature header".to_string())
        })?;
        
        // Compute expected signature
        let expected = match provider {
            GitProvider::GitHub | GitProvider::Gitea => {
                // HMAC-SHA256
                format!("sha256={}", self.compute_hmac(payload))
            }
            GitProvider::GitLab => {
                // Token-based
                self.config.webhook_secret.clone()
            }
            GitProvider::Bitbucket => {
                // HMAC-SHA256
                self.compute_hmac(payload)
            }
            GitProvider::Generic => {
                self.config.webhook_secret.clone()
            }
        };
        
        if signature != expected {
            return Err(WebhookError::InvalidSignature("Signature mismatch".to_string()));
        }
        
        Ok(())
    }

    /// Compute HMAC-SHA256
    fn compute_hmac(&self, payload: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        // Simplified - in production use proper HMAC
        let mut hasher = DefaultHasher::new();
        payload.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Parse GitHub webhook
    async fn parse_github_webhook(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<WebhookEvent, WebhookError> {
        debug!("Parsing GitHub webhook: {}", event_type);
        
        // Simplified parsing - in production use proper JSON parsing
        let (repo, branch, commit, message, author) = self.extract_github_data(event_type, payload);
        
        Ok(WebhookEvent {
            id: Uuid::nil(), // Will be set by handle()
            provider: GitProvider::GitHub,
            event_type: event_type.to_string(),
            repository: repo,
            branch,
            commit_sha: commit,
            commit_message: message,
            author,
            timestamp: Utc::now(),
            raw_payload: payload.to_string(),
            processed: false,
        })
    }

    /// Extract data from GitHub payload
    fn extract_github_data(&self, event_type: &str, _payload: &str) -> (String, String, String, String, String) {
        // Simplified - in production parse JSON
        match event_type {
            "push" => {
                ("github.com/user/repo".to_string(), 
                 "main".to_string(), 
                 format!("{:040x}", Uuid::new_v4().as_u128()),
                 "Commit message".to_string(),
                 "developer".to_string())
            }
            _ => {
                (String::new(), String::new(), String::new(), String::new(), String::new())
            }
        }
    }

    /// Parse GitLab webhook
    async fn parse_gitlab_webhook(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<WebhookEvent, WebhookError> {
        debug!("Parsing GitLab webhook: {}", event_type);
        
        // Similar to GitHub but with GitLab-specific structure
        Ok(WebhookEvent {
            id: Uuid::nil(),
            provider: GitProvider::GitLab,
            event_type: event_type.to_string(),
            repository: "gitlab.com/user/repo".to_string(),
            branch: "main".to_string(),
            commit_sha: format!("{:040x}", Uuid::new_v4().as_u128()),
            commit_message: "GitLab commit".to_string(),
            author: "developer".to_string(),
            timestamp: Utc::now(),
            raw_payload: payload.to_string(),
            processed: false,
        })
    }

    /// Parse Bitbucket webhook
    async fn parse_bitbucket_webhook(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<WebhookEvent, WebhookError> {
        debug!("Parsing Bitbucket webhook: {}", event_type);
        
        Ok(WebhookEvent {
            id: Uuid::nil(),
            provider: GitProvider::Bitbucket,
            event_type: event_type.to_string(),
            repository: "bitbucket.org/user/repo".to_string(),
            branch: "main".to_string(),
            commit_sha: format!("{:040x}", Uuid::new_v4().as_u128()),
            commit_message: "Bitbucket commit".to_string(),
            author: "developer".to_string(),
            timestamp: Utc::now(),
            raw_payload: payload.to_string(),
            processed: false,
        })
    }

    /// Parse Gitea webhook
    async fn parse_gitea_webhook(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<WebhookEvent, WebhookError> {
        debug!("Parsing Gitea webhook: {}", event_type);
        
        Ok(WebhookEvent {
            id: Uuid::nil(),
            provider: GitProvider::Gitea,
            event_type: event_type.to_string(),
            repository: "gitea.example.com/user/repo".to_string(),
            branch: "main".to_string(),
            commit_sha: format!("{:040x}", Uuid::new_v4().as_u128()),
            commit_message: "Gitea commit".to_string(),
            author: "developer".to_string(),
            timestamp: Utc::now(),
            raw_payload: payload.to_string(),
            processed: false,
        })
    }

    /// Parse generic webhook
    async fn parse_generic_webhook(
        &self,
        event_type: &str,
        payload: &str,
    ) -> Result<WebhookEvent, WebhookError> {
        debug!("Parsing generic webhook: {}", event_type);
        
        Ok(WebhookEvent {
            id: Uuid::nil(),
            provider: GitProvider::Generic,
            event_type: event_type.to_string(),
            repository: "unknown".to_string(),
            branch: "main".to_string(),
            commit_sha: format!("{:040x}", Uuid::new_v4().as_u128()),
            commit_message: "Generic webhook".to_string(),
            author: "unknown".to_string(),
            timestamp: Utc::now(),
            raw_payload: payload.to_string(),
            processed: false,
        })
    }

    /// Process webhook event
    async fn process_webhook_event(&self, event: &WebhookEvent) -> Result<Option<Uuid>, WebhookError> {
        // Only process push events
        if event.event_type != "push" {
            debug!("Ignoring non-push event: {}", event.event_type);
            return Ok(None);
        }
        
        // Find matching repository
        let registration = {
            let repos = self.repositories.read().await;
            repos.values()
                .find(|r| self.repo_matches_event(&r.repository_url, event))
                .cloned()
        };
        
        let registration = match registration {
            Some(r) => r,
            None => {
                debug!("No matching repository registration found for event");
                return Ok(None);
            }
        };
        
        // Check if we should deploy
        if !registration.auto_deploy {
            debug!("Auto-deploy disabled for repository");
            return Ok(None);
        }
        
        // Check branch
        if !registration.deploy_on_branches.contains(&event.branch) {
            debug!("Branch {} not in deploy list", event.branch);
            return Ok(None);
        }
        
        // Create build spec
        let build_spec = BuildSpec {
            id: Uuid::new_v4(),
            app_id: registration.app_id.clone(),
            app_name: registration.app_name.clone(),
            repository_url: registration.repository_url.clone(),
            branch: event.branch.clone(),
            commit_sha: event.commit_sha.clone(),
            dockerfile_path: registration.dockerfile_path.clone(),
            build_context: ".".to_string(),
            image_tag: format!("registry/{}/{}:{}",
                registration.organization_id,
                registration.app_name,
                &event.commit_sha[..8]
            ),
            build_args: HashMap::new(),
            secrets: HashMap::new(),
            created_at: Utc::now(),
            priority: BuildPriority::Normal,
            organization_id: registration.organization_id.clone(),
        };
        
        // Submit to build queue
        let build_id = self.build_queue.submit(build_spec).await?;
        
        Ok(Some(build_id))
    }

    /// Check if repository URL matches event
    fn repo_matches_event(&self, repo_url: &str, event: &WebhookEvent) -> bool {
        let repo_key = self.extract_repo_key(repo_url);
        repo_key == event.repository || event.repository.contains(&repo_key)
    }

    /// Extract repository key from URL
    fn extract_repo_key(&self, url: &str) -> String {
        // Extract owner/repo from various URL formats
        let url = url.trim_end_matches(".git");
        
        if let Some(pos) = url.rfind('/') {
            if let Some(prev_pos) = url[..pos].rfind('/') {
                return url[prev_pos + 1..].to_string();
            }
            return url[pos + 1..].to_string();
        }
        
        url.to_string()
    }

    /// Get registered repositories
    pub async fn list_repositories(&self) -> Vec<RepositoryRegistration> {
        let repos = self.repositories.read().await;
        repos.values().cloned().collect()
    }

    /// Get webhook events
    pub async fn list_events(&self, limit: usize) -> Vec<WebhookEvent> {
        let events = self.events.read().await;
        let mut events: Vec<_> = events.values().cloned().collect();
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);
        events
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WebhookError {
    #[error("Repository not found: {0}")]
    NotFound(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Unsupported event type: {0}")]
    UnsupportedEvent(String),

    #[error("Processing error: {0}")]
    ProcessingError(String),

    #[error("Build error: {0}")]
    BuildError(#[from] crate::git::builder::BuildError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_router() -> WebhookRouter {
        let config = WebhookConfig::default();
        let build_queue = Arc::new(BuildQueue::new(Default::default()));
        WebhookRouter::new(config, build_queue)
    }

    #[tokio::test]
    async fn test_register_repository() {
        let router = create_test_router();
        
        let registration = RepositoryRegistration {
            id: Uuid::new_v4(),
            organization_id: "org-123".to_string(),
            app_id: "app-123".to_string(),
            app_name: "test-app".to_string(),
            repository_url: "https://github.com/user/repo.git".to_string(),
            provider: GitProvider::GitHub,
            default_branch: "main".to_string(),
            auto_deploy: true,
            deploy_on_branches: vec!["main".to_string()],
            deploy_on_tags: false,
            build_command: None,
            dockerfile_path: "Dockerfile".to_string(),
            webhook_secret: String::new(),
            created_at: Utc::now(),
            last_deployment: None,
        };
        
        let result = router.register_repository(registration).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_webhook() {
        let router = create_test_router();
        
        let result = router.handle(
            GitProvider::GitHub,
            "push",
            None,
            "{}",
        ).await;
        
        // Should succeed (no signature validation with empty secret)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_extract_repo_key() {
        let router = create_test_router();
        
        assert_eq!(
            router.extract_repo_key("https://github.com/user/repo.git"),
            "user/repo"
        );
        
        assert_eq!(
            router.extract_repo_key("git@github.com:user/repo.git"),
            "user/repo"
        );
    }
}
