//! Git integration for push-to-deploy workflows
//! 
//! Webhook receivers and build job queuing.

pub mod webhook;
pub mod builder;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Webhook validation failed: {0}")]
    WebhookValidation(String),
    
    #[error("Clone failed: {0}")]
    CloneFailed(String),
    
    #[error("Build failed: {0}")]
    BuildFailed(String),
    
    #[error("Repository not found: {0}")]
    RepoNotFound(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Git service coordinator
pub struct GitService {
    // TODO: Add webhook_router, build_queue, repo_cache
}

impl GitService {
    /// Initialize git service
    pub async fn new(config: &GitConfig) -> Result<Self, GitError> {
        // TODO: Setup webhook secret validation
        // TODO: Initialize build queue
        // TODO: Prepare repo cache directory
        unimplemented!("GitService::new")
    }

    /// Register repository for webhooks
    pub async fn register_repo(&self, repo: &Repository) -> Result<WebhookUrl, GitError> {
        // TODO: Generate unique webhook URL
        // TODO: Store repo config
        // TODO: Return URL for user to configure
        unimplemented!("GitService::register_repo")
    }

    /// Unregister repository
    pub async fn unregister_repo(&self, repo_id: &str) -> Result<(), GitError> {
        // TODO: Remove from registry
        // TODO: Cleanup cached data
        unimplemented!("GitService::unregister_repo")
    }

    /// Trigger manual build
    pub async fn trigger_build(&self, repo_id: &str, ref_name: &str) -> Result<BuildId, GitError> {
        // TODO: Lookup repo
        // TODO: Queue build job
        // TODO: Return build ID
        unimplemented!("GitService::trigger_build")
    }

    /// Get build status
    pub async fn build_status(&self, build_id: &str) -> Result<BuildInfo, GitError> {
        // TODO: Query build queue
        // TODO: Return current status and logs
        unimplemented!("GitService::build_status")
    }

    /// Stream build logs
    pub async fn stream_logs(
        &self,
        build_id: &str,
        sender: &mut dyn LogSender,
    ) -> Result<(), GitError> {
        // TODO: Subscribe to build log topic
        // TODO: Forward to sender
        unimplemented!("GitService::stream_logs")
    }

    /// List recent builds
    pub async fn list_builds(&self, repo_id: &str, limit: usize) -> Result<Vec<BuildInfo>, GitError> {
        // TODO: Query build history
        // TODO: Return paginated results
        unimplemented!("GitService::list_builds")
    }
}

/// Repository configuration
#[derive(Debug, Clone)]
pub struct Repository {
    // TODO: Add id, provider (github/gitlab), owner, name, clone_url
    // TODO: Add default_branch, deploy_branch_pattern
    // TODO: Add build_config (dockerfile_path, build_args)
}

/// Webhook URL for configuration
#[derive(Debug, Clone)]
pub struct WebhookUrl {
    // TODO: Add url, secret
}

/// Build identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildId(String);

/// Build information
#[derive(Debug, Clone)]
pub struct BuildInfo {
    // TODO: Add id, repo_id, ref_name, commit_sha, status
    // TODO: Add started_at, finished_at, logs_url
}

/// Build status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatus {
    Queued,
    Cloning,
    Building,
    Pushing,
    Succeeded,
    Failed,
    Cancelled,
}

/// Git configuration
#[derive(Debug, Clone)]
pub struct GitConfig {
    // TODO: Add webhook_base_url, repo_cache_path
    // TODO: Add build_concurrency, default_timeout
    // TODO: Add registry_push_url, registry_auth
}

/// Log sender trait
pub trait LogSender: Send {
    // TODO: fn send(&mut self, line: &str) -> Result<(), Error>;
}