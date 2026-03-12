//! Build queue and executor
//!
//! Manages build jobs and executes them using Docker/Buildkit.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Build queue configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildQueueConfig {
    /// Maximum concurrent builds
    pub max_concurrent_builds: usize,
    /// Build timeout in seconds
    pub build_timeout_secs: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Build log directory
    pub log_dir: String,
    /// Docker socket path
    pub docker_socket: String,
    /// Buildkit address (optional)
    pub buildkit_addr: Option<String>,
}

impl Default for BuildQueueConfig {
    fn default() -> Self {
        Self {
            max_concurrent_builds: 4,
            build_timeout_secs: 600, // 10 minutes
            max_retries: 3,
            log_dir: "/var/lib/shellwego/builds/logs".to_string(),
            docker_socket: "/var/run/docker.sock".to_string(),
            buildkit_addr: None,
        }
    }
}

/// Build specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSpec {
    pub id: Uuid,
    pub app_id: String,
    pub app_name: String,
    pub repository_url: String,
    pub branch: String,
    pub commit_sha: String,
    pub dockerfile_path: String,
    pub build_context: String,
    pub image_tag: String,
    pub build_args: HashMap<String, String>,
    pub secrets: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub priority: BuildPriority,
    pub organization_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

/// Build result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub build_id: Uuid,
    pub status: BuildStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub image_digest: Option<String>,
    pub image_size_bytes: Option<u64>,
    pub log_path: Option<String>,
    pub error_message: Option<String>,
    pub build_duration_secs: Option<u64>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildStatus {
    Queued,
    Running,
    Success,
    Failed,
    Cancelled,
    Timeout,
}

/// Build job
#[derive(Debug, Clone)]
struct BuildJob {
    spec: BuildSpec,
    result: BuildResult,
    retry_count: u32,
}

/// Build queue
pub struct BuildQueue {
    config: BuildQueueConfig,
    pending: Arc<RwLock<Vec<BuildJob>>>,
    running: Arc<RwLock<HashMap<Uuid, BuildJob>>>,
    completed: Arc<RwLock<HashMap<Uuid, BuildResult>>>,
    sender: mpsc::Sender<BuildSpec>,
    semaphore: Arc<Semaphore>,
}

impl BuildQueue {
    /// Create a new build queue
    pub fn new(config: BuildQueueConfig) -> Self {
        let (sender, mut receiver) = mpsc::channel::<BuildSpec>(100);
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_builds));
        
        let queue = Self {
            config,
            pending: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(HashMap::new())),
            completed: Arc::new(RwLock::new(HashMap::new())),
            sender,
            semaphore,
        };
        
        // Start queue processor
        let pending = queue.pending.clone();
        let running = queue.running.clone();
        let semaphore = queue.semaphore.clone();
        
        tokio::spawn(async move {
            while let Some(spec) = receiver.recv().await {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                
                let pending_clone = pending.clone();
                let running_clone = running.clone();
                
                tokio::spawn(async move {
                    // Move spec to running
                    let job = {
                        let mut pending = pending_clone.write().await;
                        pending.iter()
                            .position(|j| j.spec.id == spec.id)
                            .map(|i| pending.remove(i))
                    };
                    
                    if let Some(mut job) = job {
                        job.result.status = BuildStatus::Running;
                        job.result.started_at = Some(Utc::now());
                        
                        {
                            let mut running = running_clone.write().await;
                            running.insert(spec.id, job.clone());
                        }
                        
                        // Execute build (simulated)
                        let executor = BuildExecutor::new(Default::default());
                        let result = executor.execute(&job.spec).await;
                        
                        // Move to completed
                        {
                            let mut running = running_clone.write().await;
                            running.remove(&spec.id);
                        }
                        
                        {
                            let mut completed = queue.completed.write().await;
                            completed.insert(spec.id, result);
                        }
                    }
                    
                    drop(permit);
                });
            }
        });
        
        queue
    }

    /// Submit a build to the queue
    pub async fn submit(&self, spec: BuildSpec) -> Result<Uuid, BuildError> {
        let build_id = spec.id;
        
        let job = BuildJob {
            spec: spec.clone(),
            result: BuildResult {
                build_id,
                status: BuildStatus::Queued,
                started_at: None,
                completed_at: None,
                image_digest: None,
                image_size_bytes: None,
                log_path: None,
                error_message: None,
                build_duration_secs: None,
                metadata: HashMap::new(),
            },
            retry_count: 0,
        };
        
        // Add to pending queue
        {
            let mut pending = self.pending.write().await;
            // Insert sorted by priority
            let pos = pending.iter()
                .position(|j| j.spec.priority < spec.priority)
                .unwrap_or(pending.len());
            pending.insert(pos, job);
        }
        
        // Send to processor
        self.sender.send(spec).await
            .map_err(|e| BuildError::QueueError(e.to_string()))?;
        
        info!("Build {} submitted to queue (priority: {:?})", build_id, spec.priority);
        Ok(build_id)
    }

    /// Cancel a build
    pub async fn cancel(&self, build_id: &Uuid) -> Result<(), BuildError> {
        // Check pending queue
        {
            let mut pending = self.pending.write().await;
            if let Some(pos) = pending.iter().position(|j| j.spec.id == *build_id) {
                pending.remove(pos);
                info!("Build {} cancelled from pending queue", build_id);
                return Ok(());
            }
        }
        
        // Check running builds
        {
            let mut running = self.running.write().await;
            if let Some(job) = running.get_mut(build_id) {
                job.result.status = BuildStatus::Cancelled;
                job.result.completed_at = Some(Utc::now());
                info!("Build {} marked for cancellation", build_id);
            }
        }
        
        Err(BuildError::NotFound(*build_id))
    }

    /// Get build status
    pub async fn get_status(&self, build_id: &Uuid) -> Option<BuildResult> {
        // Check pending
        {
            let pending = self.pending.read().await;
            if let Some(job) = pending.iter().find(|j| j.spec.id == *build_id) {
                return Some(job.result.clone());
            }
        }
        
        // Check running
        {
            let running = self.running.read().await;
            if let Some(job) = running.get(build_id) {
                return Some(job.result.clone());
            }
        }
        
        // Check completed
        {
            let completed = self.completed.read().await;
            completed.get(build_id).cloned()
        }
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> BuildQueueStats {
        let pending = self.pending.read().await;
        let running = self.running.read().await;
        let completed = self.completed.read().await;
        
        BuildQueueStats {
            pending_count: pending.len(),
            running_count: running.len(),
            completed_count: completed.len(),
            successful_count: completed.values().filter(|r| r.status == BuildStatus::Success).count(),
            failed_count: completed.values().filter(|r| r.status == BuildStatus::Failed).count(),
        }
    }

    /// List pending builds
    pub async fn list_pending(&self) -> Vec<BuildSpec> {
        let pending = self.pending.read().await;
        pending.iter().map(|j| j.spec.clone()).collect()
    }

    /// List recent builds
    pub async fn list_recent(&self, limit: usize) -> Vec<BuildResult> {
        let completed = self.completed.read().await;
        let mut results: Vec<_> = completed.values().cloned().collect();
        results.sort_by(|a, b| {
            b.completed_at.cmp(&a.completed_at)
        });
        results.truncate(limit);
        results
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueueStats {
    pub pending_count: usize,
    pub running_count: usize,
    pub completed_count: usize,
    pub successful_count: usize,
    pub failed_count: usize,
}

/// Build executor
pub struct BuildExecutor {
    config: BuildQueueConfig,
}

impl BuildExecutor {
    /// Create a new build executor
    pub fn new(config: BuildQueueConfig) -> Self {
        Self { config }
    }

    /// Execute a build
    pub async fn execute(&self, spec: &BuildSpec) -> BuildResult {
        let started_at = Utc::now();
        info!("Executing build {} for app {}", spec.id, spec.app_name);
        
        let mut result = BuildResult {
            build_id: spec.id,
            status: BuildStatus::Running,
            started_at: Some(started_at),
            completed_at: None,
            image_digest: None,
            image_size_bytes: None,
            log_path: None,
            error_message: None,
            build_duration_secs: None,
            metadata: HashMap::new(),
        };
        
        // Create build log directory
        let log_dir = PathBuf::from(&self.config.log_dir);
        let log_path = log_dir.join(format!("{}.log", spec.id));
        result.log_path = Some(log_path.to_string_lossy().to_string());
        
        // Execute build phases
        match self.execute_phases(spec).await {
            Ok((digest, size)) => {
                result.status = BuildStatus::Success;
                result.image_digest = Some(digest);
                result.image_size_bytes = Some(size);
                info!("Build {} completed successfully", spec.id);
            }
            Err(e) => {
                result.status = BuildStatus::Failed;
                result.error_message = Some(e.to_string());
                error!("Build {} failed: {}", spec.id, e);
            }
        }
        
        let completed_at = Utc::now();
        result.completed_at = Some(completed_at);
        result.build_duration_secs = Some((completed_at - started_at).num_seconds() as u64);
        
        result
    }

    /// Execute build phases
    async fn execute_phases(&self, spec: &BuildSpec) -> Result<(String, u64), BuildError> {
        // Phase 1: Clone repository
        self.clone_repo(spec).await?;
        
        // Phase 2: Detect build configuration
        self.detect_build_config(spec).await?;
        
        // Phase 3: Build image
        self.build_dockerfile(spec).await?;
        
        // Phase 4: Push to registry
        let (digest, size) = self.push_image(spec).await?;
        
        Ok((digest, size))
    }

    /// Clone repository
    async fn clone_repo(&self, spec: &BuildSpec) -> Result<(), BuildError> {
        debug!("Cloning repository: {} (branch: {})", spec.repository_url, spec.branch);
        
        // Simulate git clone
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // In production, would use git2 or command:
        // git clone --branch {branch} --depth 1 {url} {context}
        
        debug!("Repository cloned successfully: {}", spec.commit_sha);
        Ok(())
    }

    /// Detect build configuration
    async fn detect_build_config(&self, spec: &BuildSpec) -> Result<(), BuildError> {
        debug!("Detecting build configuration for {}", spec.app_name);
        
        // Check for various build configurations
        // - Dockerfile
        // - docker-compose.yml
        // - Procfile
        // - Buildpacks
        
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        Ok(())
    }

    /// Build Dockerfile
    async fn build_dockerfile(&self, spec: &BuildSpec) -> Result<(), BuildError> {
        info!("Building Dockerfile for {} at {}", spec.app_name, spec.dockerfile_path);
        
        // Simulate Docker build
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // In production, would use Docker API or buildkit:
        // docker build -t {image_tag} -f {dockerfile_path} {build_context}
        
        info!("Docker build completed for {}", spec.image_tag);
        Ok(())
    }

    /// Push image to registry
    async fn push_image(&self, spec: &BuildSpec) -> Result<(String, u64), BuildError> {
        info!("Pushing image {}", spec.image_tag);
        
        // Simulate push
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Generate simulated digest and size
        let digest = format!("sha256:{:064x}", spec.id);
        let size = 1024 * 1024 * 50; // 50 MB
        
        info!("Image pushed successfully: {}", digest);
        Ok((digest, size))
    }

    /// Run build with timeout
    pub async fn execute_with_timeout(&self, spec: &BuildSpec) -> BuildResult {
        let timeout = Duration::from_secs(self.config.build_timeout_secs);
        
        match tokio::time::timeout(timeout, self.execute(spec)).await {
            Ok(result) => result,
            Err(_) => {
                let mut result = BuildResult {
                    build_id: spec.id,
                    status: BuildStatus::Timeout,
                    started_at: Some(Utc::now()),
                    completed_at: Some(Utc::now()),
                    image_digest: None,
                    image_size_bytes: None,
                    log_path: None,
                    error_message: Some(format!("Build timed out after {} seconds", self.config.build_timeout_secs)),
                    build_duration_secs: Some(self.config.build_timeout_secs),
                    metadata: HashMap::new(),
                };
                result.log_path = Some(format!("{}/{}.log", self.config.log_dir, spec.id));
                result
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("Build not found: {0}")]
    NotFound(Uuid),
    
    #[error("Clone failed: {0}")]
    CloneFailed(String),
    
    #[error("Build failed: {0}")]
    BuildFailed(String),
    
    #[error("Push failed: {0}")]
    PushFailed(String),
    
    #[error("Timeout")]
    Timeout,
    
    #[error("Queue error: {0}")]
    QueueError(String),
    
    #[error("Docker error: {0}")]
    DockerError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_submit_build() {
        let queue = BuildQueue::new(BuildQueueConfig::default());
        
        let spec = BuildSpec {
            id: Uuid::new_v4(),
            app_id: "app-123".to_string(),
            app_name: "test-app".to_string(),
            repository_url: "https://github.com/test/app.git".to_string(),
            branch: "main".to_string(),
            commit_sha: "abc123".to_string(),
            dockerfile_path: "Dockerfile".to_string(),
            build_context: ".".to_string(),
            image_tag: "registry/test-app:latest".to_string(),
            build_args: HashMap::new(),
            secrets: HashMap::new(),
            created_at: Utc::now(),
            priority: BuildPriority::Normal,
            organization_id: "org-123".to_string(),
        };
        
        let build_id = queue.submit(spec).await.unwrap();
        assert!(!build_id.is_nil());
    }

    #[tokio::test]
    async fn test_build_executor() {
        let executor = BuildExecutor::new(BuildQueueConfig::default());
        
        let spec = BuildSpec {
            id: Uuid::new_v4(),
            app_id: "app-456".to_string(),
            app_name: "executor-test".to_string(),
            repository_url: "https://github.com/test/executor.git".to_string(),
            branch: "main".to_string(),
            commit_sha: "def456".to_string(),
            dockerfile_path: "Dockerfile".to_string(),
            build_context: ".".to_string(),
            image_tag: "registry/executor-test:latest".to_string(),
            build_args: HashMap::new(),
            secrets: HashMap::new(),
            created_at: Utc::now(),
            priority: BuildPriority::Normal,
            organization_id: "org-123".to_string(),
        };
        
        let result = executor.execute(&spec).await;
        assert_eq!(result.status, BuildStatus::Success);
        assert!(result.image_digest.is_some());
    }

    #[tokio::test]
    async fn test_queue_stats() {
        let queue = BuildQueue::new(BuildQueueConfig::default());
        
        let stats = queue.get_stats().await;
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.running_count, 0);
    }
}
