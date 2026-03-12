//! Logger and metrics configuration types

use serde::{Serialize, Deserialize};

/// Logger configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Logger {
    /// Path to the named pipe or file for log output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// Log level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    /// Whether to output the level in logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_level: Option<bool>,
    /// Whether to include file path and line number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_log_origin: Option<bool>,
    /// Module path to filter log messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// Log level enumeration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum LogLevel {
    Error,
    Warning,
    #[default]
    Info,
    Debug,
    Trace,
    Off,
}

/// Metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Metrics {
    /// Path to the named pipe or file for metrics output.
    pub metrics_path: String,
}
