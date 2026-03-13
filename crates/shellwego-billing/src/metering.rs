//! Usage metering and aggregation
//!
//! This module provides high-throughput usage tracking for billing.
//! It includes:
//! - Persistent time-series storage for usage data
//! - Real-time in-memory counters for dashboard displays
//! - Aggregation at multiple granularities
//! - Query capabilities for billing calculations

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc, Duration, Timelike, Datelike};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions, Row};
use tracing::{info, warn, instrument};

use crate::{BillingError, UsageEvent};

/// Time-series metrics store
/// 
/// Stores usage events in a time-series optimized format for
/// efficient querying and aggregation. Uses PostgreSQL with
/// TimescaleDB extension for production deployments.
pub struct MetricsStore {
    /// Database connection pool
    pool: Option<PgPool>,
    /// In-memory buffer for high-throughput ingestion
    buffer: Arc<DashMap<String, Vec<BufferedEvent>>>,
    /// Buffer flush interval in seconds
    flush_interval_secs: u64,
    /// Maximum buffer size before forced flush
    max_buffer_size: usize,
}

/// Buffered usage event for batch insertion
#[derive(Debug, Clone)]
struct BufferedEvent {
    customer_id: String,
    resource_type: String,
    quantity: f64,
    timestamp: DateTime<Utc>,
    metadata: HashMap<String, String>,
}

impl MetricsStore {
    /// Initialize metrics store
    /// 
    /// Connects to the time-series database and ensures required schema exists.
    /// Supports PostgreSQL/TimescaleDB, SQLite for development, and can fall
    /// back to in-memory storage for testing.
    #[instrument(skip(dsn))]
    pub async fn new(dsn: &str) -> Result<Self, BillingError> {
        info!(dsn = %dsn, "Initializing metrics store");
        
        let pool = if dsn.starts_with("sqlite:") {
            // For SQLite, we use in-memory buffer primarily
            None
        } else {
            // PostgreSQL/TimescaleDB connection
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(dsn)
                .await
                .map_err(|e| BillingError::MeteringError(format!("Database connection failed: {}", e)))?;
            
            // Ensure schema exists
            Self::ensure_schema(&pool).await?;
            
            Some(pool)
        };
        
        let buffer = Arc::new(DashMap::new());
        
        info!("Metrics store initialized successfully");
        
        Ok(Self {
            pool,
            buffer,
            flush_interval_secs: 60,
            max_buffer_size: 10000,
        })
    }
    
    /// Create schema for metrics storage
    async fn ensure_schema(pool: &PgPool) -> Result<(), BillingError> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS usage_events (
                id BIGSERIAL,
                customer_id VARCHAR(255) NOT NULL,
                resource_type VARCHAR(100) NOT NULL,
                quantity DOUBLE PRECISION NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL,
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (id, timestamp)
            );
            
            -- Create hypertable for time-series optimization (TimescaleDB)
            -- SELECT create_hypertable('usage_events', 'timestamp', if_not_exists => TRUE);
            
            -- Indexes for common query patterns
            CREATE INDEX IF NOT EXISTS idx_usage_events_customer_time 
                ON usage_events (customer_id, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_usage_events_resource 
                ON usage_events (resource_type, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_usage_events_customer_resource 
                ON usage_events (customer_id, resource_type, timestamp DESC);
        "#)
        .execute(pool)
        .await
        .map_err(|e| BillingError::MeteringError(format!("Schema creation failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Insert a usage event
    /// 
    /// Writes to an in-memory buffer for high-throughput ingestion.
    /// The buffer is periodically flushed to persistent storage.
    #[instrument(skip(self, event), fields(customer_id = %event.customer_id, resource = %event.resource_type))]
    pub async fn insert(&self, event: &UsageEvent) -> Result<(), BillingError> {
        // Validate event
        if event.customer_id.is_empty() {
            return Err(BillingError::MeteringError("Customer ID is required".to_string()));
        }
        
        if event.resource_type.is_empty() {
            return Err(BillingError::MeteringError("Resource type is required".to_string()));
        }
        
        // Add to buffer
        let key = format!("{}:{}", event.customer_id, event.resource_type);
        let buffered = BufferedEvent {
            customer_id: event.customer_id.clone(),
            resource_type: event.resource_type.clone(),
            quantity: event.quantity,
            timestamp: event.timestamp,
            metadata: event.metadata.clone(),
        };
        
        self.buffer
            .entry(key)
            .or_insert_with(Vec::new)
            .push(buffered);
        
        // Check if we need to flush
        let total_size: usize = self.buffer.iter().map(|e| e.len()).sum();
        if total_size >= self.max_buffer_size {
            self.flush().await?;
        }
        
        Ok(())
    }
    
    /// Flush buffer to persistent storage
    pub async fn flush(&self) -> Result<(), BillingError> {
        if let Some(pool) = &self.pool {
            let events: Vec<BufferedEvent> = self.buffer
                .iter()
                .flat_map(|entry| entry.value().clone())
                .collect();

            if events.is_empty() {
                return Ok(());
            }

            let events_count = events.len();

            // Batch insert
            let mut tx = pool.begin().await
                .map_err(|e| BillingError::MeteringError(format!("Transaction start failed: {}", e)))?;

            for event in events {
                let metadata_json = serde_json::to_value(&event.metadata)
                    .unwrap_or(serde_json::Value::Object(Default::default()));

                sqlx::query(r#"
                    INSERT INTO usage_events (customer_id, resource_type, quantity, timestamp, metadata)
                    VALUES ($1, $2, $3, $4, $5)
                "#)
                .bind(&event.customer_id)
                .bind(&event.resource_type)
                .bind(event.quantity)
                .bind(event.timestamp)
                .bind(metadata_json)
                .execute(&mut *tx)
                .await
                .map_err(|e| BillingError::MeteringError(format!("Insert failed: {}", e)))?;
            }

            tx.commit().await
                .map_err(|e| BillingError::MeteringError(format!("Transaction commit failed: {}", e)))?;

            // Clear buffer
            self.buffer.clear();

            info!(count = events_count, "Flushed usage events to database");
        }

        Ok(())
    }
    
    /// Query aggregated usage data
    /// 
    /// Retrieves usage data aggregated at the specified granularity.
    /// Returns time-series data points suitable for billing calculations
    /// or dashboard visualization.
    #[instrument(skip(self), fields(customer_id = %customer_id, resource = %resource_type))]
    pub async fn query(
        &self,
        customer_id: &str,
        resource_type: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        granularity: Granularity,
    ) -> Result<Vec<DataPoint>, BillingError> {
        if start >= end {
            return Err(BillingError::MeteringError("Start time must be before end time".to_string()));
        }
        
        if let Some(pool) = &self.pool {
            let query_str = format!(r#"
                SELECT 
                    date_trunc('{}', timestamp) as time_bucket,
                    SUM(quantity) as total_quantity
                FROM usage_events
                WHERE customer_id = $1
                  AND resource_type = $2
                  AND timestamp >= $3
                  AND timestamp < $4
                GROUP BY time_bucket
                ORDER BY time_bucket ASC
            "#, Self::granularity_to_trunc(&granularity));
            
            let rows = sqlx::query(&query_str)
                .bind(customer_id)
                .bind(resource_type)
                .bind(start)
                .bind(end)
                .fetch_all(pool)
                .await
                .map_err(|e| BillingError::MeteringError(format!("Query failed: {}", e)))?;
            
            let data_points: Vec<DataPoint> = rows
                .into_iter()
                .map(|row| {
                    let timestamp: DateTime<Utc> = row.get("time_bucket");
                    let total: f64 = row.get("total_quantity");
                    DataPoint {
                        timestamp,
                        value: total,
                    }
                })
                .collect();
            
            Ok(data_points)
        } else {
            // In-memory fallback for SQLite/testing
            let mut points = Vec::new();
            let mut current = start;
            
            while current < end {
                let duration = Self::granularity_duration(&granularity);
                let next = current + duration;
                
                // Sum from buffer for this time window
                let total: f64 = self.buffer
                    .iter()
                    .filter(|entry| {
                        let key = entry.key();
                        key.starts_with(&format!("{}:{}", customer_id, resource_type))
                    })
                    .flat_map(|entry| entry.value().clone())
                    .filter(|event| event.timestamp >= current && event.timestamp < next)
                    .map(|event| event.quantity)
                    .sum();
                
                if total > 0.0 {
                    points.push(DataPoint {
                        timestamp: current,
                        value: total,
                    });
                }
                
                current = next;
            }
            
            Ok(points)
        }
    }
    
    /// Get all resource types for a customer
    pub async fn get_resource_types(&self, customer_id: &str) -> Result<Vec<String>, BillingError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query(r#"
                SELECT DISTINCT resource_type
                FROM usage_events
                WHERE customer_id = $1
            "#)
            .bind(customer_id)
            .fetch_all(pool)
            .await
            .map_err(|e| BillingError::MeteringError(format!("Query failed: {}", e)))?;
            
            Ok(rows.into_iter().map(|row| row.get("resource_type")).collect())
        } else {
            // In-memory fallback
            let resource_types: Vec<String> = self.buffer
                .iter()
                .filter(|entry| entry.key().starts_with(&format!("{}:", customer_id)))
                .filter_map(|entry| {
                    entry.key().split(':').nth(1).map(|s| s.to_string())
                })
                .collect();
            
            Ok(resource_types)
        }
    }
    
    /// Get current month's running total for a customer
    /// 
    /// Returns a map of resource types to total usage for the current
    /// billing period. Useful for real-time dashboards and alerts.
    #[instrument(skip(self), fields(customer_id = %customer_id))]
    pub async fn current_month_total(&self, customer_id: &str) -> Result<HashMap<String, f64>, BillingError> {
        let now = Utc::now();
        let month_start = now
            .with_day(1)
            .unwrap()
            .with_hour(0)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap();
        
        if let Some(pool) = &self.pool {
            let rows = sqlx::query(r#"
                SELECT resource_type, SUM(quantity) as total
                FROM usage_events
                WHERE customer_id = $1
                  AND timestamp >= $2
                GROUP BY resource_type
            "#)
            .bind(customer_id)
            .bind(month_start)
            .fetch_all(pool)
            .await
            .map_err(|e| BillingError::MeteringError(format!("Query failed: {}", e)))?;
            
            let mut totals = HashMap::new();
            for row in rows {
                let resource_type: String = row.get("resource_type");
                let total: f64 = row.get("total");
                totals.insert(resource_type, total);
            }
            
            // Add buffered events not yet flushed
            for entry in self.buffer.iter() {
                if entry.key().starts_with(&format!("{}:", customer_id)) {
                    if let Some(resource_type) = entry.key().split(':').nth(1) {
                        let buffered_total: f64 = entry.value()
                            .iter()
                            .filter(|e| e.timestamp >= month_start)
                            .map(|e| e.quantity)
                            .sum();
                        
                        *totals.entry(resource_type.to_string()).or_insert(0.0) += buffered_total;
                    }
                }
            }
            
            Ok(totals)
        } else {
            // In-memory fallback
            let mut totals = HashMap::new();
            
            for entry in self.buffer.iter() {
                if entry.key().starts_with(&format!("{}:", customer_id)) {
                    if let Some(resource_type) = entry.key().split(':').nth(1) {
                        let total: f64 = entry.value()
                            .iter()
                            .filter(|e| e.timestamp >= month_start)
                            .map(|e| e.quantity)
                            .sum();
                        
                        *totals.entry(resource_type.to_string()).or_insert(0.0) += total;
                    }
                }
            }
            
            Ok(totals)
        }
    }
    
    /// Delete old usage data beyond retention period
    pub async fn cleanup_old_data(&self, retention_days: u32) -> Result<u64, BillingError> {
        let cutoff = Utc::now() - Duration::days(retention_days as i64);
        
        if let Some(pool) = &self.pool {
            let result = sqlx::query(r#"
                DELETE FROM usage_events
                WHERE timestamp < $1
            "#)
            .bind(cutoff)
            .execute(pool)
            .await
            .map_err(|e| BillingError::MeteringError(format!("Cleanup failed: {}", e)))?;
            
            let deleted = result.rows_affected();
            info!(deleted, retention_days, "Cleaned up old usage data");
            Ok(deleted)
        } else {
            Ok(0)
        }
    }
    
    // Helper methods
    
    fn granularity_to_trunc(granularity: &Granularity) -> &'static str {
        match granularity {
            Granularity::Raw => "second",
            Granularity::Minute => "minute",
            Granularity::Hour => "hour",
            Granularity::Day => "day",
            Granularity::Month => "month",
        }
    }
    
    fn granularity_duration(granularity: &Granularity) -> Duration {
        match granularity {
            Granularity::Raw => Duration::seconds(1),
            Granularity::Minute => Duration::minutes(1),
            Granularity::Hour => Duration::hours(1),
            Granularity::Day => Duration::days(1),
            Granularity::Month => Duration::days(30),
        }
    }
}

/// Data point in time series
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp of the data point
    pub timestamp: DateTime<Utc>,
    /// Value at this point
    pub value: f64,
}

impl DataPoint {
    /// Create a new data point
    pub fn new(timestamp: DateTime<Utc>, value: f64) -> Self {
        Self { timestamp, value }
    }
}

/// Aggregation granularity
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Granularity {
    /// Raw data points (no aggregation)
    Raw,
    /// Minute-level aggregation
    Minute,
    /// Hour-level aggregation
    Hour,
    /// Day-level aggregation
    Day,
    /// Month-level aggregation
    Month,
}

/// Real-time usage counter (in-memory)
/// 
/// Thread-safe counter for tracking usage in real-time.
/// Used for dashboard displays and rate limiting.
pub struct RealtimeCounter {
    /// Nested map: customer_id -> resource_type -> count
    counters: DashMap<String, DashMap<String, f64>>,
}

impl RealtimeCounter {
    /// Create a new real-time counter
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
        }
    }
    
    /// Increment a counter atomically
    /// 
    /// Thread-safe increment operation. Uses DashMap for
    /// fine-grained locking.
    pub fn increment(&self, customer_id: &str, resource: &str, amount: f64) {
        let customer_counters = self.counters
            .entry(customer_id.to_string())
            .or_insert_with(DashMap::new);
        
        customer_counters
            .entry(resource.to_string())
            .and_modify(|count| *count += amount)
            .or_insert(amount);
    }
    
    /// Get current value for a specific counter
    pub fn get(&self, customer_id: &str, resource: &str) -> f64 {
        self.counters
            .get(customer_id)
            .and_then(|resources| resources.get(resource).map(|r| *r))
            .unwrap_or(0.0)
    }
    
    /// Get all counters for a customer
    pub fn get_customer(&self, customer_id: &str) -> HashMap<String, f64> {
        self.counters
            .get(customer_id)
            .map(|resources| {
                resources.iter()
                    .map(|entry| (entry.key().clone(), *entry.value()))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Flush and reset counters for a customer
    /// 
    /// Returns current values and resets them to zero.
    /// Used when flushing to persistent storage.
    pub fn flush(&self, customer_id: &str) -> HashMap<String, f64> {
        if let Some((_, resources)) = self.counters.remove(customer_id) {
            resources.iter()
                .map(|entry| (entry.key().clone(), *entry.value()))
                .collect()
        } else {
            HashMap::new()
        }
    }
    
    /// Flush all counters
    /// 
    /// Returns all current counter values and resets them.
    pub fn flush_all(&self) -> HashMap<String, HashMap<String, f64>> {
        let mut result = HashMap::new();
        
        for entry in self.counters.iter() {
            let customer_id = entry.key().clone();
            let resources = entry.value();
            
            let resource_map: HashMap<String, f64> = resources.iter()
                .map(|r| (r.key().clone(), *r.value()))
                .collect();
            
            result.insert(customer_id, resource_map);
        }
        
        // Clear all counters
        self.counters.clear();
        
        result
    }
    
    /// Get total usage across all customers
    pub fn total_usage(&self) -> f64 {
        self.counters
            .iter()
            .flat_map(|entry| {
                let values: Vec<f64> = entry.value().iter().map(|r| *r.value()).collect();
                values
            })
            .sum()
    }
    
    /// Get number of active customers
    pub fn active_customers(&self) -> usize {
        self.counters.len()
    }
}

impl Default for RealtimeCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_realtime_counter_increment() {
        let counter = RealtimeCounter::new();
        
        counter.increment("cust_1", "cpu_hours", 5.0);
        counter.increment("cust_1", "cpu_hours", 3.0);
        counter.increment("cust_1", "memory_gb", 10.0);
        counter.increment("cust_2", "cpu_hours", 2.0);
        
        assert_eq!(counter.get("cust_1", "cpu_hours"), 8.0);
        assert_eq!(counter.get("cust_1", "memory_gb"), 10.0);
        assert_eq!(counter.get("cust_2", "cpu_hours"), 2.0);
        assert_eq!(counter.get("cust_3", "cpu_hours"), 0.0);
    }
    
    #[test]
    fn test_realtime_counter_flush() {
        let counter = RealtimeCounter::new();
        
        counter.increment("cust_1", "cpu_hours", 5.0);
        counter.increment("cust_1", "memory_gb", 10.0);
        
        let flushed = counter.flush("cust_1");
        
        assert_eq!(flushed.get("cpu_hours"), Some(&5.0));
        assert_eq!(flushed.get("memory_gb"), Some(&10.0));
        assert_eq!(counter.get("cust_1", "cpu_hours"), 0.0);
    }
    
    #[test]
    fn test_realtime_counter_flush_all() {
        let counter = RealtimeCounter::new();
        
        counter.increment("cust_1", "cpu_hours", 5.0);
        counter.increment("cust_2", "cpu_hours", 3.0);
        
        let all = counter.flush_all();
        
        assert_eq!(all.len(), 2);
        assert_eq!(counter.active_customers(), 0);
    }
    
    #[test]
    fn test_realtime_counter_total_usage() {
        let counter = RealtimeCounter::new();
        
        counter.increment("cust_1", "cpu_hours", 5.0);
        counter.increment("cust_1", "memory_gb", 10.0);
        counter.increment("cust_2", "cpu_hours", 3.0);
        
        assert_eq!(counter.total_usage(), 18.0);
    }
    
    #[tokio::test]
    async fn test_metrics_store_in_memory() {
        let store = MetricsStore::new("sqlite::memory:").await.unwrap();
        
        let event = UsageEvent::new("cust_1", "cpu_hours", 5.0);
        store.insert(&event).await.unwrap();
        
        let totals = store.current_month_total("cust_1").await.unwrap();
        assert_eq!(totals.get("cpu_hours"), Some(&5.0));
    }
    
    #[test]
    fn test_data_point_creation() {
        let now = Utc::now();
        let point = DataPoint::new(now, 42.5);
        
        assert_eq!(point.timestamp, now);
        assert_eq!(point.value, 42.5);
    }
}
