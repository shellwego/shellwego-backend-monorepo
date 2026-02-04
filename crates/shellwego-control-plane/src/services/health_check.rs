//! Application health checking service

use shellwego_core::entities::app::{App, HealthCheck};

/// Health check runner
pub struct HealthChecker {
    // TODO: Add http_client, check_interval
}

impl HealthChecker {
    /// Create health checker
    pub fn new() -> Self {
        // TODO: Initialize with config
        unimplemented!("HealthChecker::new")
    }

    /// Start checking an app
    pub async fn start_checking(&self, app: &App) -> CheckHandle {
        // TODO: Spawn background task
        // TODO: Periodic health checks
        // TODO: Report results to control plane
        unimplemented!("HealthChecker::start_checking")
    }

    /// Stop checking an app
    pub async fn stop_checking(&self, handle: CheckHandle) {
        // TODO: Cancel background task
        unimplemented!("HealthChecker::stop_checking")
    }

    /// Execute single health check
    pub async fn check_once(&self, check: &HealthCheck, target: &str) -> CheckResult {
        // TODO: HTTP GET to health endpoint
        // TODO: Check response code and optionally body
        // TODO: Measure response time
        unimplemented!("HealthChecker::check_once")
    }
}

/// Health check handle
#[derive(Debug, Clone)]
pub struct CheckHandle {
    // TODO: Add cancellation token
}

/// Check result
#[derive(Debug, Clone)]
pub struct CheckResult {
    // TODO: Add healthy, response_time_ms, status_code, error_message
}