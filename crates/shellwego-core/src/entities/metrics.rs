//! Time-series metrics entities

use crate::prelude::*;

/// Metric sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub timestamp: DateTime<Utc>,
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Metric series metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub retention_days: u32,
}

/// Alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub query: String, // PromQL or similar
    pub condition: AlertCondition,
    pub duration_secs: u64, // For how long condition must be true
    pub severity: AlertSeverity,
    pub notification_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    pub comparison: ComparisonOp,
    pub threshold: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    Gt, // >
    Lt, // <
    Eq, // ==
    Ne, // !=
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
}