//! Top - Real-time resource monitoring TUI
//!
//! Displays a beautiful dashboard showing nodes, apps, and resources
//! with live updates using ratatui.

use crate::config::CliConfig;
use anyhow::Result;

pub struct TopArgs {
    /// Refresh interval in milliseconds
    #[arg(short, long, default_value = "1000")]
    pub interval: u64,

    /// Focus on specific node
    #[arg(short, long)]
    pub node: Option<String>,

    /// Show only apps
    #[arg(long)]
    pub apps_only: bool,

    /// Show only nodes
    #[arg(long)]
    pub nodes_only: bool,
}

pub async fn handle(args: TopArgs, config: &CliConfig) -> Result<()> {
    // TODO: Initialize ratatui terminal
    // TODO: Create API client from config
    // TODO: Set up signal handler for Ctrl+C
    // TODO: Enter main event loop
    // TODO: Fetch initial data (nodes, apps, resources)
    // TODO: Render initial dashboard layout
    // TODO: Process events (keyboard, resize, timer)
    // TODO: Update data on each interval tick
    // TODO: Handle node filtering if --node specified
    // TODO: Render appropriate view based on --apps-only/--nodes-only flags
    // TODO: Handle graceful exit and restore terminal
    Ok(())
}

mod ui {
    use ratatui::prelude::*;

    pub struct DashboardState {
        // TODO: Store nodes data
        // TODO: Store apps data
        // TODO: Store resource metrics (CPU, memory, network)
        // TODO: Store selected item index
        // TODO: Store current sort column
    }

    impl DashboardState {
        // TODO: pub fn new() -> Self
        // TODO: pub fn update(&mut self, data: &ApiData)
        // TODO: pub fn next_item(&mut self)
        // TODO: pub fn prev_item(&mut self)
    }

    pub fn render(state: &DashboardState, frame: &mut Frame<'_>) {
        // TODO: Render header with title and stats
        // TODO: Render nodes table with status indicators
        // TODO: Render apps panel with resource usage
        // TODO: Render resource charts (CPU, memory, network)
        // TODO: Render footer with help hints
    }
}

mod data {
    pub struct ApiData {
        // TODO: Vec<Node>
        // TODO: Vec<App>
        // TODO: SystemMetrics
    }

    pub async fn fetch(api_client: &ApiClient) -> Result<ApiData> {
        // TODO: Fetch nodes from /api/v1/nodes
        // TODO: Fetch apps from /api/v1/apps
        // TODO: Fetch metrics from /api/v1/metrics
        // TODO: Combine into ApiData struct
    }
}

struct ApiClient {
    // TODO: Base URL
    // TODO: Auth token
}

impl ApiClient {
    // TODO: fn new(url: &str, token: &str) -> Self
    // TODO: async fn fetch_nodes(&self) -> Result<Vec<Node>>
    // TODO: async fn fetch_apps(&self) -> Result<Vec<App>>
    // TODO: async fn fetch_metrics(&self) -> Result<SystemMetrics>
}

struct Node {
    // TODO: id
    // TODO: name
    // TODO: status (online, offline, unknown)
    // TODO: cpu_usage
    // TODO: memory_usage
    // TODO: region
}

struct App {
    // TODO: id
    // TODO: name
    // TODO: status
    // TODO: replicas
    // TODO: cpu_usage
    // TODO: memory_usage
}

struct SystemMetrics {
    // TODO: total_nodes
    // TODO: online_nodes
    // TODO: total_apps
    // TODO: running_apps
    // TODO: cpu_usage_avg
    // TODO: memory_usage_avg
}
