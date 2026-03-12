//! Command handlers

pub mod apps;
pub mod auth;
pub mod databases;
pub mod domains;
pub mod exec;
pub mod logs;
pub mod nodes;
pub mod secrets;
pub mod status;
pub mod top;
pub mod update;
pub mod volumes;

use crate::OutputFormat;
use comfy_table::{Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};

/// Create styled table for terminal output
pub fn create_table() -> Table {
    let mut table = Table::new();
    table
        .set_header(vec!["Property", "Value"])
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table
}

/// Format output based on user preference
pub fn format_output<T: serde::Serialize>(data: &T, format: OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(data)?),
        OutputFormat::Yaml => Ok(serde_yaml::to_string(data)?),
        OutputFormat::Plain => Ok(format!("{:?}", data)), // Debug fallback
        OutputFormat::Table => Err(anyhow::anyhow!("Table format requires manual construction")),
    }
}