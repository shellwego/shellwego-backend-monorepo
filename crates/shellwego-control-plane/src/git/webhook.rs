//! Webhook receivers for GitHub, GitLab, Gitea

use crate::git::{GitError, GitService, Repository, BuildId};

/// Webhook handler router
pub struct WebhookRouter {
    // TODO: Add git_service, secret_validator
}

impl WebhookRouter {
    /// Create router
    pub fn new(service: &GitService) -> Self {
        // TODO: Store service reference
        unimplemented!("WebhookRouter::new")
    }

    /// Handle incoming webhook POST
    pub async fn handle(
        &self,
        provider: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<WebhookResult, GitError> {
        // TODO: Validate signature
        // TODO: Parse payload based on provider
        // TODO: Trigger build if push to tracked branch
        unimplemented!("WebhookRouter::handle")
    }

    /// Validate GitHub signature
    fn validate_github(headers: &[(String, String)], body: &[u8], secret: &str) -> Result<(), GitError> {
        // TODO: Compute HMAC-SHA256
        // TODO: Compare with X-Hub-Signature-256
        unimplemented!("WebhookRouter::validate_github")
    }

    /// Validate GitLab token
    fn validate_gitlab(headers: &[(String, String)], secret: &str) -> Result<(), GitError> {
        // TODO: Compare X-Gitlab-Token
        unimplemented!("WebhookRouter::validate_gitlab")
    }

    /// Parse GitHub push event
    fn parse_github_push(body: &[u8]) -> Result<PushEvent, GitError> {
        // TODO: Deserialize JSON
        // TODO: Extract ref, commit, repo info
        unimplemented!("WebhookRouter::parse_github_push")
    }

    /// Parse GitLab push event
    fn parse_gitlab_push(body: &[u8]) -> Result<PushEvent, GitError> {
        // TODO: Deserialize JSON
        unimplemented!("WebhookRouter::parse_gitlab_push")
    }
}

/// Webhook processing result
#[derive(Debug, Clone)]
pub struct WebhookResult {
    // TODO: Add processed, build_id, message
}

/// Parsed push event
#[derive(Debug, Clone)]
pub struct PushEvent {
    // TODO: Add repo_owner, repo_name, ref_name, commit_sha, commit_message, author
}

/// Pull request event (for PR previews)
#[derive(Debug, Clone)]
pub struct PullRequestEvent {
    // TODO: Add action, number, branch, base_branch, author
}