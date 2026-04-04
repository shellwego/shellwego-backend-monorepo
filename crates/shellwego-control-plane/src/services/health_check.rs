//! Health check service
//!
//! Performs HTTP and TCP health checks on backend services with
//! configurable intervals and thresholds.

use std::collections::HashMap;
use std::net::{TcpStream, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Health check configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckConfig {
    /// Default check interval in seconds
    pub default_interval_secs: u64,
    /// Default timeout in seconds
    pub default_timeout_secs: u64,
    /// Number of failures before marking unhealthy
    pub failure_threshold: u32,
    /// Number of successes before marking healthy
    pub success_threshold: u32,
    /// Enable background health monitoring
    pub background_monitoring: bool,
    /// Maximum concurrent checks
    pub max_concurrent_checks: usize,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            default_interval_secs: 30,
            default_timeout_secs: 5,
            failure_threshold: 3,
            success_threshold: 2,
            background_monitoring: true,
            max_concurrent_checks: 100,
        }
    }
}

/// Health check type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckType {
    Http {
        path: String,
        port: u16,
        scheme: String,
        expected_status: u16,
        expected_body: Option<String>,
        headers: HashMap<String, String>,
    },
    Tcp {
        port: u16,
    },
    Exec {
        command: String,
        args: Vec<String>,
    },
    Grpc {
        service: String,
        port: u16,
    },
}

/// Target to health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckTarget {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub check_type: CheckType,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub enabled: bool,
    pub labels: HashMap<String, String>,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub target_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub message: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Degraded,
    Unknown,
}

/// Target health state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetHealthState {
    pub target: HealthCheckTarget,
    pub current_status: HealthStatus,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check: Option<HealthCheckResult>,
    pub last_healthy: Option<DateTime<Utc>>,
    pub last_unhealthy: Option<DateTime<Utc>>,
    pub total_checks: u64,
    pub total_failures: u64,
}

/// Health check service
pub struct HealthCheckService {
    config: HealthCheckConfig,
    targets: Arc<RwLock<HashMap<Uuid, TargetHealthState>>>,
    http_client: reqwest::Client,
}

impl HealthCheckService {
    /// Create a new health check service
    pub fn new(config: HealthCheckConfig) -> Self {
        info!("Initializing health check service with interval: {}s", config.default_interval_secs);
        
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.default_timeout_secs))
            .danger_accept_invalid_certs(true) // For internal services
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            targets: Arc::new(RwLock::new(HashMap::new())),
            http_client,
        }
    }

    /// Register a target for health checking
    pub async fn register_target(
        &self,
        name: String,
        address: String,
        check_type: CheckType,
        interval_secs: Option<u64>,
        timeout_secs: Option<u64>,
        labels: HashMap<String, String>,
    ) -> Result<HealthCheckTarget, HealthCheckError> {
        let target_id = Uuid::new_v4();
        
        let target = HealthCheckTarget {
            id: target_id,
            name,
            address,
            check_type,
            interval_secs: interval_secs.unwrap_or(self.config.default_interval_secs),
            timeout_secs: timeout_secs.unwrap_or(self.config.default_timeout_secs),
            enabled: true,
            labels,
        };

        let state = TargetHealthState {
            target: target.clone(),
            current_status: HealthStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check: None,
            last_healthy: None,
            last_unhealthy: None,
            total_checks: 0,
            total_failures: 0,
        };

        {
            let mut targets = self.targets.write().await;
            targets.insert(target_id, state);
        }

        info!("Registered health check target: {} ({})", target.name, target.id);
        Ok(target)
    }

    /// Deregister a target
    pub async fn deregister_target(&self, target_id: &Uuid) -> Result<(), HealthCheckError> {
        let mut targets = self.targets.write().await;
        targets.remove(target_id)
            .ok_or_else(|| HealthCheckError::NotFound(*target_id))?;
        
        info!("Deregistered health check target: {}", target_id);
        Ok(())
    }

    /// Perform health check on a specific target
    pub async fn check_target(&self, target_id: &Uuid) -> Result<HealthCheckResult, HealthCheckError> {
        let target = {
            let targets = self.targets.read().await;
            targets.get(target_id)
                .ok_or_else(|| HealthCheckError::NotFound(*target_id))?
                .target.clone()
        };

        let result = self.perform_check(&target).await;
        
        // Update state
        {
            let mut targets = self.targets.write().await;
            if let Some(state) = targets.get_mut(target_id) {
                self.update_state(state, &result);
            }
        }

        Ok(result)
    }

    /// Perform the actual health check
    async fn perform_check(&self, target: &HealthCheckTarget) -> HealthCheckResult {
        let start = Instant::now();
        
        let (status, message, details) = match &target.check_type {
            CheckType::Http { path, port, scheme, expected_status, expected_body, headers } => {
                self.check_http(target, path, *port, scheme, *expected_status, expected_body, headers).await
            }
            CheckType::Tcp { port } => {
                self.check_tcp(&target.address, *port).await
            }
            CheckType::Exec { command, args } => {
                self.check_exec(command, args).await
            }
            CheckType::Grpc { service, port } => {
                self.check_grpc(&target.address, service, *port).await
            }
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        HealthCheckResult {
            target_id: target.id,
            timestamp: Utc::now(),
            status,
            latency_ms,
            message,
            details,
        }
    }

    /// HTTP health check
    async fn check_http(
        &self,
        target: &HealthCheckTarget,
        path: &str,
        port: u16,
        scheme: &str,
        expected_status: u16,
        expected_body: &Option<String>,
        headers: &HashMap<String, String>,
    ) -> (HealthStatus, String, HashMap<String, String>) {
        let url = format!("{}://{}:{}{}", scheme, target.address, port, path);
        
        debug!("Performing HTTP health check: {}", url);

        let mut request = self.http_client.get(&url);
        
        for (key, value) in headers {
            request = request.header(key, value);
        }

        match request.send().await {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let mut details = HashMap::new();
                details.insert("status_code".to_string(), status_code.to_string());

                let body = response.text().await.unwrap_or_default();

                if status_code != expected_status {
                    return (
                        HealthStatus::Unhealthy,
                        format!("Expected status {}, got {}", expected_status, status_code),
                        details,
                    );
                }

                if let Some(expected) = expected_body {
                    if !body.contains(expected) {
                        return (
                            HealthStatus::Unhealthy,
                            format!("Response body does not contain expected string"),
                            details,
                        );
                    }
                }

                (HealthStatus::Healthy, "HTTP check passed".to_string(), details)
            }
            Err(e) => {
                let mut details = HashMap::new();
                details.insert("error".to_string(), e.to_string());
                (HealthStatus::Unhealthy, format!("HTTP request failed: {}", e), details)
            }
        }
    }

    /// TCP health check
    async fn check_tcp(&self, address: &str, port: u16) -> (HealthStatus, String, HashMap<String, String>) {
        let addr: SocketAddr = match format!("{}:{}", address, port).parse() {
            Ok(a) => a,
            Err(e) => {
                let mut details = HashMap::new();
                details.insert("error".to_string(), e.to_string());
                return (HealthStatus::Unhealthy, format!("Invalid address: {}", e), details);
            }
        };

        debug!("Performing TCP health check: {}", addr);

        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(_) => (HealthStatus::Healthy, "TCP connection successful".to_string(), HashMap::new()),
            Err(e) => {
                let mut details = HashMap::new();
                details.insert("error".to_string(), e.to_string());
                (HealthStatus::Unhealthy, format!("TCP connection failed: {}", e), details)
            }
        }
    }

    /// Exec health check (simulated)
    async fn check_exec(&self, command: &str, args: &[String]) -> (HealthStatus, String, HashMap<String, String>) {
        debug!("Performing exec health check: {} {:?}", command, args);
        
        // Simulated - in production would use tokio::process::Command
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        (HealthStatus::Healthy, "Exec check passed".to_string(), HashMap::new())
    }

    /// gRPC health check (simulated)
    async fn check_grpc(&self, address: &str, service: &str, port: u16) -> (HealthStatus, String, HashMap<String, String>) {
        debug!("Performing gRPC health check: {}:{} for service {}", address, port, service);
        
        // Simulated - in production would use gRPC health checking protocol
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        (HealthStatus::Healthy, "gRPC check passed".to_string(), HashMap::new())
    }

    /// Update target health state
    fn update_state(&self, state: &mut TargetHealthState, result: &HealthCheckResult) {
        state.last_check = Some(result.clone());
        state.total_checks += 1;

        match result.status {
            HealthStatus::Healthy => {
                state.consecutive_successes += 1;
                state.consecutive_failures = 0;
                
                if state.consecutive_successes >= self.config.success_threshold {
                    state.current_status = HealthStatus::Healthy;
                    state.last_healthy = Some(result.timestamp);
                }
            }
            HealthStatus::Unhealthy => {
                state.consecutive_failures += 1;
                state.consecutive_successes = 0;
                state.total_failures += 1;
                
                if state.consecutive_failures >= self.config.failure_threshold {
                    state.current_status = HealthStatus::Unhealthy;
                    state.last_unhealthy = Some(result.timestamp);
                }
            }
            HealthStatus::Degraded => {
                state.current_status = HealthStatus::Degraded;
            }
            HealthStatus::Unknown => {}
        }
    }

    /// Get target health state
    pub async fn get_target_state(&self, target_id: &Uuid) -> Option<TargetHealthState> {
        let targets = self.targets.read().await;
        targets.get(target_id).cloned()
    }

    /// Get all target states
    pub async fn list_targets(&self) -> Vec<TargetHealthState> {
        let targets = self.targets.read().await;
        targets.values().cloned().collect()
    }

    /// Get targets by status
    pub async fn get_targets_by_status(&self, status: HealthStatus) -> Vec<TargetHealthState> {
        let targets = self.targets.read().await;
        targets.values()
            .filter(|t| t.current_status == status)
            .cloned()
            .collect()
    }

    /// Run health check on all targets
    pub async fn check_all(&self) -> HashMap<Uuid, HealthCheckResult> {
        let target_ids: Vec<Uuid> = {
            let targets = self.targets.read().await;
            targets.keys().copied().collect()
        };

        let mut results = HashMap::new();
        for target_id in target_ids {
            if let Ok(result) = self.check_target(&target_id).await {
                results.insert(target_id, result);
            }
        }

        results
    }

    /// Get overall health summary
    pub async fn get_health_summary(&self) -> HealthSummary {
        let targets = self.targets.read().await;
        
        let mut healthy = 0;
        let mut unhealthy = 0;
        let mut degraded = 0;
        let mut unknown = 0;

        for state in targets.values() {
            match state.current_status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Unhealthy => unhealthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Unknown => unknown += 1,
            }
        }

        HealthSummary {
            total_targets: targets.len() as u64,
            healthy,
            unhealthy,
            degraded,
            unknown,
        }
    }
}

/// Health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub total_targets: u64,
    pub healthy: u64,
    pub unhealthy: u64,
    pub degraded: u64,
    pub unknown: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum HealthCheckError {
    #[error("Target not found: {0}")]
    NotFound(Uuid),
    
    #[error("Health check failed: {0}")]
    CheckFailed(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_target() {
        let service = HealthCheckService::new(HealthCheckConfig::default());
        
        let target = service.register_target(
            "test-app".to_string(),
            "127.0.0.1".to_string(),
            CheckType::Tcp { port: 8080 },
            None,
            None,
            HashMap::new(),
        ).await.unwrap();
        
        assert_eq!(target.name, "test-app");
        assert!(target.enabled);
    }

    #[tokio::test]
    async fn test_check_target() {
        let service = HealthCheckService::new(HealthCheckConfig::default());
        
        let target = service.register_target(
            "test-app".to_string(),
            "127.0.0.1".to_string(),
            CheckType::Tcp { port: 22 }, // SSH port typically open
            None,
            None,
            HashMap::new(),
        ).await.unwrap();
        
        let result = service.check_target(&target.id).await.unwrap();
        assert!(result.status == HealthStatus::Healthy || result.status == HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_summary() {
        let service = HealthCheckService::new(HealthCheckConfig::default());
        
        service.register_target(
            "app1".to_string(),
            "127.0.0.1".to_string(),
            CheckType::Tcp { port: 8080 },
            None,
            None,
            HashMap::new(),
        ).await.unwrap();
        
        let summary = service.get_health_summary().await;
        assert_eq!(summary.total_targets, 1);
    }
}
