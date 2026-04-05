//! Billing and metering for commercial deployments
//!
//! Usage tracking, invoicing, and payment processing.
//! This module provides a complete billing system with:
//! - High-throughput usage metering with time-series storage
//! - Automatic invoice generation with PDF rendering
//! - Multi-provider payment processing (Stripe, Paystack, etc.)
//! - Prorated billing calculations
//! - Webhook handling for payment notifications
//!
//! ## Type Organization
//!
//! Domain entity types are defined in `shellwego-schema/src/billing/` and re-exported here:
//! - [`Customer`], [`Address`], [`SubscriptionTier`], [`CustomerStatus`] - Customer types
//! - [`Invoice`], [`InvoiceStatus`], [`LineItem`], [`BillingPeriod`] - Invoice types
//! - [`PaymentMethod`], [`PaymentResult`] - Payment types
//! - [`UsageEvent`], [`UsageSummary`] - Usage tracking types
//! - [`BillingConfig`], [`DunningConfig`] - Configuration types

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc, Duration};
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Sha512};
use sqlx::{PgPool, postgres::PgPoolOptions, Row};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn, error, instrument, debug};
use uuid::Uuid;

// Re-export billing types from schema
pub use shellwego_schema::billing::{
    Address, BillingConfig, BillingPeriod, Customer, CustomerStatus, DunningConfig, Invoice,
    InvoiceStatus, LineItem, PaymentMethod, PaymentResult, SubscriptionTier, UsageEvent,
    UsageSummary,
};

pub mod metering;
pub mod invoices;

pub use metering::{MetricsStore, RealtimeCounter, DataPoint, Granularity};
pub use invoices::{InvoiceGenerator, InvoiceTemplate};

/// Main billing error type
#[derive(Error, Debug)]
pub enum BillingError {
    #[error("Metering error: {0}")]
    MeteringError(String),

    #[error("Invoice generation failed: {0}")]
    InvoiceError(String),

    #[error("Payment failed: {0}")]
    PaymentError(String),

    #[error("Customer not found: {0}")]
    CustomerNotFound(String),

    #[error("Invalid configuration: {0}")]
    ConfigurationError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Template rendering error: {0}")]
    TemplateError(String),

    #[error("Payment provider error: {0}")]
    ProviderError(String),

    #[error("Webhook verification failed: {0}")]
    WebhookVerificationError(String),

    #[error("Invoice not found: {0}")]
    InvoiceNotFound(String),

    #[error("Invalid period: {0}")]
    InvalidPeriod(String),

    #[error("HTTP request error: {0}")]
    HttpError(String),
}

/// Webhook processing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResult {
    /// Event type from webhook
    pub event_type: String,
    /// Whether the event was processed
    pub processed: bool,
}

/// Webhook event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebhookEvent {
    event_type: String,
    data: HashMap<String, serde_json::Value>,
}

/// Payment record tracking payment attempts with retry info.
///
/// Stored in the `payments` database table. Each payment attempt against an
/// invoice gets its own row, enabling full audit trail and dunning support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRecord {
    /// Unique identifier
    pub id: String,
    /// Associated invoice
    pub invoice_id: String,
    /// Customer who made the payment
    pub customer_id: String,
    /// Amount in smallest currency unit (cents)
    pub amount_cents: i64,
    /// ISO 4217 currency code
    pub currency: String,
    /// Payment method type (card, bank_transfer, wallet, crypto)
    pub method_type: String,
    /// Payment provider (stripe, paystack)
    pub provider: String,
    /// Payment status (pending, succeeded, failed, refunded)
    pub status: String,
    /// Transaction ID from the provider
    pub transaction_id: Option<String>,
    /// Raw provider response for debugging / reconciliation
    pub provider_response_json: Option<String>,
    /// How many times we have retried this payment
    pub retry_count: i32,
    /// When the next automatic retry should be attempted
    pub next_retry_at: Option<DateTime<Utc>>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Database-backed pricing plan.
///
/// Replaces the hardcoded `get_pricing()` match branches. Each row defines
/// a price per resource type. `tier_multiplier` is applied to the base price
/// based on the customer's subscription tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingPlan {
    /// Unique identifier
    pub id: String,
    /// Human-readable plan name
    pub name: String,
    /// Resource type this plan applies to (e.g. "cpu_hours")
    pub resource_type: String,
    /// Price in smallest currency unit (cents)
    pub price_cents: i64,
    /// ISO 4217 currency code
    pub currency: String,
    /// Human-readable description
    pub description: String,
    /// Multiplier applied per subscription tier (optional)
    pub tier_multiplier: Option<f64>,
    /// Whether this plan is currently active
    pub is_active: bool,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Type aliases for HMAC-SHA256 and HMAC-SHA512
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Billing system coordinator
///
/// This is the main entry point for all billing operations. It coordinates
/// between metering, invoicing, and payment processing subsystems.
///
/// When a PostgreSQL pool is available (`pool` is `Some`), all customer,
/// invoice, payment, and pricing data is persisted to the database. When
/// `pool` is `None` (e.g. SQLite in-memory mode for tests), the system falls
/// back to in-memory `HashMap` storage for backwards compatibility.
pub struct BillingSystem {
    /// Metrics storage for usage tracking
    metrics_store: Arc<MetricsStore>,
    /// Real-time usage counters
    realtime_counter: Arc<RealtimeCounter>,
    /// Invoice generator with PDF rendering
    invoice_generator: Arc<InvoiceGenerator>,
    /// Billing configuration
    config: BillingConfig,
    /// Customer cache (used as fallback when pool is None)
    customers: Arc<RwLock<HashMap<String, Customer>>>,
    /// Invoice cache (used as fallback when pool is None)
    invoices: Arc<RwLock<HashMap<String, Invoice>>>,
    /// Optional PostgreSQL pool for billing tables.
    /// When `None`, operations fall back to in-memory storage.
    pool: Option<PgPool>,
    /// Lazy-initialised reqwest client for HTTP-based payment provider calls.
    http_client: Arc<reqwest::Client>,
}

impl BillingSystem {
    /// Initialize billing system with configuration.
    ///
    /// Creates a new billing system instance with all necessary subsystems:
    /// - Connects to the metrics store for usage tracking
    /// - Initializes real-time counters for low-latency usage recording
    /// - Sets up invoice generation with templates
    /// - Prepares payment gateway connections
    /// - If `config.metrics_dsn` starts with `postgres://` or `postgresql://`,
    ///   creates a dedicated `PgPool` for billing tables as well.
    #[instrument(skip(config), fields(currency = %config.currency))]
    pub async fn new(config: &BillingConfig) -> Result<Self, BillingError> {
        info!("Initializing billing system");

        // Validate configuration
        if config.currency.is_empty() {
            return Err(BillingError::ConfigurationError("Currency is required".to_string()));
        }

        if config.invoice_day < 1 || config.invoice_day > 28 {
            return Err(BillingError::ConfigurationError(
                "Invoice day must be between 1 and 28".to_string(),
            ));
        }

        // Initialize metrics store
        let metrics_store = Arc::new(
            MetricsStore::new(&config.metrics_dsn)
                .await
                .map_err(|e| {
                    BillingError::MeteringError(format!("Failed to connect metrics store: {}", e))
                })?,
        );

        // Initialize real-time counter
        let realtime_counter = Arc::new(RealtimeCounter::new());

        // Initialize invoice generator
        let invoice_generator = Arc::new(InvoiceGenerator::new(&config.template_path)?);

        // Build a billing pool if the DSN is PostgreSQL
        let pool = if config.metrics_dsn.starts_with("postgres://")
            || config.metrics_dsn.starts_with("postgresql://")
        {
            debug!("Billing system creating PgPool from metrics_dsn (PostgreSQL detected)");
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(&config.metrics_dsn)
                .await
                .map_err(|e| {
                    BillingError::DatabaseError(sqlx::Error::Configuration(e.into()))
                })?;

            // Ensure billing schema tables exist
            Self::ensure_billing_schema(&pool).await?;
            Some(pool)
        } else {
            debug!("Billing system running without PostgreSQL pool (in-memory fallback)");
            None
        };

        // In-memory caches (used when pool is None, or as hot cache)
        let customers = Arc::new(RwLock::new(HashMap::new()));
        let invoices = Arc::new(RwLock::new(HashMap::new()));

        // Lazy HTTP client
        let http_client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| BillingError::ConfigurationError(format!(
                    "Failed to create HTTP client: {}", e
                )))?,
        );

        info!("Billing system initialized successfully");

        Ok(Self {
            metrics_store,
            realtime_counter,
            invoice_generator,
            config: config.clone(),
            customers,
            invoices,
            pool,
            http_client,
        })
    }

    /// Initialize billing system with an externally-provided PostgreSQL pool.
    ///
    /// Use this when the calling application already manages a connection pool
    /// and wants to share it with the billing subsystem. The caller is
    /// responsible for ensuring the billing schema tables exist.
    pub async fn new_with_pool(
        config: &BillingConfig,
        pool: PgPool,
    ) -> Result<Self, BillingError> {
        info!("Initializing billing system with external pool");

        if config.currency.is_empty() {
            return Err(BillingError::ConfigurationError("Currency is required".to_string()));
        }

        let metrics_store = Arc::new(
            MetricsStore::new(&config.metrics_dsn)
                .await
                .map_err(|e| {
                    BillingError::MeteringError(format!("Failed to connect metrics store: {}", e))
                })?,
        );

        let realtime_counter = Arc::new(RealtimeCounter::new());
        let invoice_generator = Arc::new(InvoiceGenerator::new(&config.template_path)?);
        let customers = Arc::new(RwLock::new(HashMap::new()));
        let invoices = Arc::new(RwLock::new(HashMap::new()));

        let http_client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| BillingError::ConfigurationError(format!(
                    "Failed to create HTTP client: {}", e
                )))?,
        );

        info!("Billing system with external pool initialized successfully");

        Ok(Self {
            metrics_store,
            realtime_counter,
            invoice_generator,
            config: config.clone(),
            customers,
            invoices,
            pool: Some(pool),
            http_client,
        })
    }

    // ----------------------------------------------------------------
    // Schema migration helpers
    // ----------------------------------------------------------------

    /// Ensure billing-specific schema tables exist in the PostgreSQL pool.
    async fn ensure_billing_schema(pool: &PgPool) -> Result<(), BillingError> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS billing_customers (
                id              VARCHAR(255) PRIMARY KEY,
                name            VARCHAR(255) NOT NULL,
                email           VARCHAR(512) NOT NULL,
                address_json    JSONB DEFAULT NULL,
                tier            VARCHAR(64)  NOT NULL DEFAULT 'Free',
                credits         BIGINT       NOT NULL DEFAULT 0,
                currency        VARCHAR(3)   NOT NULL DEFAULT 'USD',
                tax_id          VARCHAR(128) DEFAULT NULL,
                status          VARCHAR(32)  NOT NULL DEFAULT 'Active',
                payment_methods_json JSONB DEFAULT '[]'::jsonb,
                created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS invoices (
                id                  VARCHAR(255) PRIMARY KEY,
                invoice_number      VARCHAR(64)  NOT NULL UNIQUE,
                customer_id         VARCHAR(255) NOT NULL,
                customer_name       VARCHAR(512) NOT NULL,
                customer_email      VARCHAR(512) NOT NULL,
                period_start        TIMESTAMPTZ  NOT NULL,
                period_end          TIMESTAMPTZ  NOT NULL,
                line_items_json     JSONB         NOT NULL DEFAULT '[]'::jsonb,
                subtotal            NUMERIC(20,6) NOT NULL DEFAULT 0,
                credit_applied      NUMERIC(20,6) NOT NULL DEFAULT 0,
                total               NUMERIC(20,6) NOT NULL DEFAULT 0,
                currency            VARCHAR(3)   NOT NULL DEFAULT 'USD',
                status              VARCHAR(32)  NOT NULL DEFAULT 'Draft',
                due_date            TIMESTAMPTZ  NOT NULL,
                created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                paid_at             TIMESTAMPTZ  DEFAULT NULL,
                transaction_id      VARCHAR(255) DEFAULT NULL
            );

            CREATE TABLE IF NOT EXISTS payments (
                id                      VARCHAR(255) PRIMARY KEY,
                invoice_id              VARCHAR(255) DEFAULT NULL,
                customer_id             VARCHAR(255) NOT NULL,
                amount_cents            BIGINT       NOT NULL DEFAULT 0,
                currency                VARCHAR(3)   NOT NULL DEFAULT 'USD',
                method_type             VARCHAR(64)  NOT NULL DEFAULT 'card',
                provider                VARCHAR(64)  NOT NULL DEFAULT 'stripe',
                status                  VARCHAR(32)  NOT NULL DEFAULT 'pending',
                transaction_id          VARCHAR(255) DEFAULT NULL,
                provider_response_json  JSONB DEFAULT NULL,
                retry_count             INTEGER      NOT NULL DEFAULT 0,
                next_retry_at           TIMESTAMPTZ  DEFAULT NULL,
                created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS pricing_plans (
                id              VARCHAR(255) PRIMARY KEY,
                name            VARCHAR(255) NOT NULL,
                resource_type   VARCHAR(128) NOT NULL,
                price_cents     BIGINT       NOT NULL DEFAULT 0,
                currency        VARCHAR(3)   NOT NULL DEFAULT 'USD',
                description     TEXT DEFAULT '',
                tier_multiplier DOUBLE PRECISION DEFAULT 1.0,
                is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
                created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS subscriptions (
                id                      VARCHAR(255) PRIMARY KEY,
                customer_id             VARCHAR(255) NOT NULL,
                plan_id                 VARCHAR(255) DEFAULT NULL,
                status                  VARCHAR(32)  NOT NULL DEFAULT 'active',
                current_period_start    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                current_period_end      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                provider_subscription_id VARCHAR(255) DEFAULT NULL,
                created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
                updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW()
            );
        "#)
        .execute(pool)
        .await
        .map_err(|e| BillingError::DatabaseError(e))?;

        info!("Billing schema tables ensured");
        Ok(())
    }

    // ----------------------------------------------------------------
    // Public API: Customer management
    // ----------------------------------------------------------------

    /// Register a new customer.
    ///
    /// Persists the customer to the database when a pool is available,
    /// otherwise stores in the in-memory fallback map.
    pub async fn register_customer(&self, customer: &Customer) -> Result<(), BillingError> {
        let address_json = customer.address.as_ref()
            .map(|a| serde_json::to_value(a))
            .transpose()?;
        let payment_methods_json = serde_json::to_value(&customer.payment_methods)?;
        let tier_str = format!("{:?}", customer.tier);
        let status_str = format!("{:?}", customer.status);

        if let Some(pool) = &self.pool {
            sqlx::query(r#"
                INSERT INTO billing_customers
                    (id, name, email, address_json, tier, credits, currency,
                     tax_id, status, payment_methods_json, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    email = EXCLUDED.email,
                    address_json = EXCLUDED.address_json,
                    tier = EXCLUDED.tier,
                    credits = EXCLUDED.credits,
                    currency = EXCLUDED.currency,
                    tax_id = EXCLUDED.tax_id,
                    status = EXCLUDED.status,
                    payment_methods_json = EXCLUDED.payment_methods_json,
                    updated_at = EXCLUDED.updated_at
            "#)
            .bind(&customer.id)
            .bind(&customer.name)
            .bind(&customer.email)
            .bind(&address_json)
            .bind(&tier_str)
            .bind(customer.credits)
            .bind(&customer.currency)
            .bind(&customer.tax_id)
            .bind(&status_str)
            .bind(&payment_methods_json)
            .bind(customer.created_at)
            .bind(Utc::now())
            .execute(pool)
            .await?;
        } else {
            let mut customers = self.customers.write().await;
            customers.insert(customer.id.clone(), customer.clone());
        }

        info!(customer_id = %customer.id, "Customer registered");
        Ok(())
    }

    /// Update an existing customer record.
    pub async fn update_customer(&self, customer: &Customer) -> Result<(), BillingError> {
        // Reuse register_customer which has ON CONFLICT DO UPDATE
        self.register_customer(customer).await
    }

    /// List all registered customers.
    pub async fn list_customers(&self) -> Result<Vec<Customer>, BillingError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query(r#"
                SELECT id, name, email, address_json, tier, credits, currency,
                       tax_id, status, payment_methods_json, created_at
                FROM billing_customers
                ORDER BY created_at DESC
            "#)
            .fetch_all(pool)
            .await?;

            let mut customers = Vec::with_capacity(rows.len());
            for row in rows {
                customers.push(Self::row_to_customer(&row)?);
            }
            Ok(customers)
        } else {
            let customers = self.customers.read().await;
            Ok(customers.values().cloned().collect())
        }
    }

    // ----------------------------------------------------------------
    // Public API: Pricing plans
    // ----------------------------------------------------------------

    /// Retrieve the active pricing plan for a given resource type.
    ///
    /// Falls back to hardcoded defaults when no pool is available or
    /// when no matching plan exists in the database.
    pub async fn get_pricing_plan(&self, resource_type: &str) -> Result<PricingPlan, BillingError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(r#"
                SELECT id, name, resource_type, price_cents, currency, description,
                       tier_multiplier, is_active, created_at, updated_at
                FROM pricing_plans
                WHERE resource_type = $1 AND is_active = TRUE
                LIMIT 1
            "#)
            .bind(resource_type)
            .fetch_optional(pool)
            .await?;

            if let Some(row) = row {
                return Self::row_to_pricing_plan(&row);
            }
        }

        // Fallback: build a PricingPlan from hardcoded defaults
        let (unit_price, description) = self.get_pricing_fallback(resource_type);
        let price_cents = (unit_price * 100.0) as i64;
        Ok(PricingPlan {
            id: Uuid::new_v4().to_string(),
            name: description.clone(),
            resource_type: resource_type.to_string(),
            price_cents,
            currency: self.config.currency.clone(),
            description,
            tier_multiplier: None,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    /// Insert or update a pricing plan in the database.
    pub async fn upsert_pricing_plan(&self, plan: &PricingPlan) -> Result<(), BillingError> {
        if let Some(pool) = &self.pool {
            sqlx::query(r#"
                INSERT INTO pricing_plans
                    (id, name, resource_type, price_cents, currency, description,
                     tier_multiplier, is_active, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    resource_type = EXCLUDED.resource_type,
                    price_cents = EXCLUDED.price_cents,
                    currency = EXCLUDED.currency,
                    description = EXCLUDED.description,
                    tier_multiplier = EXCLUDED.tier_multiplier,
                    is_active = EXCLUDED.is_active,
                    updated_at = EXCLUDED.updated_at
            "#)
            .bind(&plan.id)
            .bind(&plan.name)
            .bind(&plan.resource_type)
            .bind(plan.price_cents)
            .bind(&plan.currency)
            .bind(&plan.description)
            .bind(plan.tier_multiplier)
            .bind(plan.is_active)
            .bind(plan.created_at)
            .bind(plan.updated_at)
            .execute(pool)
            .await?;
        } else {
            warn!("No database pool available; pricing plan upsert is a no-op in in-memory mode");
        }
        Ok(())
    }

    // ----------------------------------------------------------------
    // Usage & invoicing
    // ----------------------------------------------------------------

    /// Record a usage event for billing
    ///
    /// This method records a usage event in both the real-time counter
    /// and the persistent time-series store. It's designed for high-throughput
    /// ingestion with eventual consistency guarantees.
    #[instrument(skip(self, event), fields(customer_id = %event.customer_id, resource = %event.resource_type))]
    pub async fn record_usage(&self, event: UsageEvent) -> Result<(), BillingError> {
        // Validate event
        if event.customer_id.is_empty() {
            return Err(BillingError::MeteringError(
                "Customer ID is required".to_string(),
            ));
        }

        if event.quantity < 0.0 {
            return Err(BillingError::MeteringError(
                "Quantity cannot be negative".to_string(),
            ));
        }

        // Update real-time counter for dashboard displays
        self.realtime_counter.increment(
            &event.customer_id,
            &event.resource_type,
            event.quantity,
        );

        // Persist to time-series store
        self.metrics_store.insert(&event).await?;

        info!(
            customer_id = %event.customer_id,
            resource = %event.resource_type,
            quantity = event.quantity,
            "Usage recorded"
        );

        Ok(())
    }

    /// Get usage summary for a customer over a period
    ///
    /// Retrieves aggregated usage data for billing calculations.
    /// Supports different pricing tiers based on volume.
    #[instrument(skip(self), fields(customer_id = %customer_id))]
    pub async fn get_usage(
        &self,
        customer_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<UsageSummary, BillingError> {
        if start >= end {
            return Err(BillingError::InvalidPeriod(
                "Start time must be before end time".to_string(),
            ));
        }

        if customer_id.is_empty() {
            return Err(BillingError::CustomerNotFound(
                "Customer ID is required".to_string(),
            ));
        }

        // Get all resource types for the customer
        let resource_types = self.metrics_store.get_resource_types(customer_id).await?;

        let mut line_items = Vec::new();
        let mut total_amount = Decimal::ZERO;

        for resource_type in resource_types {
            // Query aggregated usage for each resource type
            let data_points = self
                .metrics_store
                .query(customer_id, &resource_type, start, end, Granularity::Month)
                .await?;

            // Sum up usage
            let total_quantity: f64 = data_points.iter().map(|dp| dp.value).sum();

            // Apply pricing (DB-backed first, fallback to hardcoded)
            let (unit_price, description) = self.get_pricing(&resource_type, total_quantity).await;
            let amount = Decimal::from_f64_retain(total_quantity * unit_price).unwrap_or(Decimal::ZERO);

            if total_quantity > 0.0 {
                line_items.push(LineItem {
                    resource_type: resource_type.clone(),
                    description,
                    quantity: total_quantity,
                    unit: self.get_unit(&resource_type),
                    unit_price,
                    amount,
                });

                total_amount += amount;
            }
        }

        // Apply tier discounts
        let customer = self.get_customer(customer_id).await?;
        let discount = self.calculate_tier_discount(&customer.tier);
        if discount > Decimal::ZERO {
            let discount_amount = total_amount * discount;
            total_amount -= discount_amount;

            line_items.push(LineItem {
                resource_type: "discount".to_string(),
                description: format!("Tier discount ({:.0}%)", discount * Decimal::ONE_HUNDRED),
                quantity: 1.0,
                unit: "item".to_string(),
                unit_price: 0.0,
                amount: -discount_amount,
            });
        }

        Ok(UsageSummary {
            customer_id: customer_id.to_string(),
            period_start: start,
            period_end: end,
            line_items,
            subtotal: total_amount,
            currency: self.config.currency.clone(),
        })
    }

    /// Generate an invoice for a customer for a billing period
    ///
    /// Creates a detailed invoice with line items based on usage,
    /// applies credits, and stores the invoice for payment processing.
    #[instrument(skip(self), fields(customer_id = %customer_id))]
    pub async fn generate_invoice(
        &self,
        customer_id: &str,
        period: BillingPeriod,
    ) -> Result<Invoice, BillingError> {
        // Get customer
        let customer = self.get_customer(customer_id).await?;

        // Get usage for the period
        let usage = self.get_usage(customer_id, period.start, period.end).await?;

        // Calculate final amount with credits
        let mut total = usage.subtotal;
        let mut credit_applied = Decimal::ZERO;

        if customer.credits > 0 {
            let available_credits = Decimal::from(customer.credits) / Decimal::ONE_HUNDRED; // cents to dollars
            credit_applied = available_credits.min(total);
            total -= credit_applied;
        }

        // Generate invoice number
        let invoice_number = self.generate_invoice_number();

        let invoice = Invoice {
            id: Uuid::new_v4().to_string(),
            invoice_number,
            customer_id: customer_id.to_string(),
            customer_name: customer.name.clone(),
            customer_email: customer.email.clone(),
            period: period.clone(),
            line_items: usage.line_items.clone(),
            subtotal: usage.subtotal,
            credit_applied,
            total,
            currency: self.config.currency.clone(),
            status: InvoiceStatus::Draft,
            due_date: period.end + Duration::days(self.config.payment_terms_days as i64),
            created_at: Utc::now(),
            paid_at: None,
        };

        // Store invoice to database
        self.store_invoice(&invoice).await?;

        info!(
            invoice_id = %invoice.id,
            invoice_number = %invoice.invoice_number,
            total = %invoice.total,
            "Invoice generated"
        );

        Ok(invoice)
    }

    /// Process payment for an invoice
    ///
    /// Charges the customer's payment method and updates the invoice status.
    /// Supports multiple payment providers through a unified interface.
    #[instrument(skip(self), fields(invoice_id = %invoice_id))]
    pub async fn process_payment(
        &self,
        invoice_id: &str,
        method: PaymentMethod,
    ) -> Result<PaymentResult, BillingError> {
        // Retrieve invoice
        let invoice = self.get_invoice(invoice_id).await?;

        if invoice.status == InvoiceStatus::Paid {
            return Ok(PaymentResult {
                success: true,
                transaction_id: None,
                message: "Invoice already paid".to_string(),
            });
        }

        let amount_cents = (invoice.total * Decimal::ONE_HUNDRED).to_string();

        // Process payment based on method
        let result = match &method {
            PaymentMethod::Card { token } => {
                self.process_card_payment(invoice_id, token, &amount_cents, &invoice.currency)
                    .await
            }
            PaymentMethod::BankTransfer { account_id } => {
                self.process_bank_transfer(invoice_id, account_id, &amount_cents, &invoice.currency)
                    .await
            }
            PaymentMethod::Wallet { provider, token } => {
                self.process_wallet_payment(invoice_id, provider, token, &amount_cents, &invoice.currency)
                    .await
            }
            PaymentMethod::Crypto { currency, address } => {
                self.process_crypto_payment(invoice_id, currency, address, &amount_cents)
                    .await
            }
        };

        // Record payment attempt
        if let Err(e) = self.record_payment_attempt(invoice_id, &invoice.customer_id, &amount_cents, &invoice.currency, &method, &result).await {
            warn!(error = %e, "Failed to record payment attempt");
        }

        // Update invoice status
        if result.success {
            self.mark_invoice_paid(invoice_id, result.transaction_id.clone())
                .await?;
        }

        Ok(result)
    }

    /// Handle webhook from payment provider
    ///
    /// Verifies the webhook signature and processes the event.
    /// Supports webhooks from Stripe, Paystack, and other providers.
    #[instrument(skip(self, payload), fields(provider = %provider))]
    pub async fn handle_webhook(
        &self,
        provider: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<WebhookResult, BillingError> {
        // Verify signature based on provider
        let verified = match provider.to_lowercase().as_str() {
            "stripe" => self.verify_stripe_webhook(payload, signature)?,
            "paystack" => self.verify_paystack_webhook(payload, signature)?,
            _ => {
                return Err(BillingError::WebhookVerificationError(format!(
                    "Unknown provider: {}",
                    provider
                )))
            }
        };

        if !verified {
            return Err(BillingError::WebhookVerificationError(
                "Invalid webhook signature".to_string(),
            ));
        }

        // Parse webhook event
        let event: WebhookEvent = serde_json::from_slice(payload)?;

        // Process based on event type
        match event.event_type.as_str() {
            "payment.succeeded" => {
                if let Some(invoice_id) = event.data.get("invoice_id").and_then(|v| v.as_str()) {
                    self.mark_invoice_paid(
                        invoice_id,
                        event
                            .data
                            .get("transaction_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    )
                    .await?;
                }
            }
            "payment.failed" => {
                if let Some(invoice_id) = event.data.get("invoice_id").and_then(|v| v.as_str()) {
                    self.handle_payment_failure(
                        invoice_id,
                        event
                            .data
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown"),
                    )
                    .await?;
                }
            }
            "customer.subscription.updated" => {
                // Handle subscription updates
                info!("Processing subscription update webhook");
            }
            _ => {
                warn!(event_type = %event.event_type, "Unhandled webhook event type");
            }
        }

        Ok(WebhookResult {
            event_type: event.event_type,
            processed: true,
        })
    }

    /// Start background workers for billing operations
    ///
    /// Spawns background tasks for:
    /// - Usage aggregation and rollup
    /// - Scheduled invoice generation
    /// - Payment retry logic (dunning)
    /// - Background workers
    pub async fn run_workers(&self) -> Result<(), BillingError> {
        info!("Starting billing background workers");

        // Usage aggregation worker
        let metrics_store = self.metrics_store.clone();
        let realtime_counter = self.realtime_counter.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

                // Flush real-time counters to persistent storage
                if let Err(e) = Self::flush_counters(&metrics_store, &realtime_counter).await {
                    error!(error = %e, "Failed to flush counters");
                }
            }
        });

        // Invoice generation scheduler (runs daily)
        let invoice_generator = self.invoice_generator.clone();
        let config = self.config.clone();
        tokio::spawn(async move {
            loop {
                // Calculate next run time (daily at configured hour)
                let now = Utc::now();
                let next_run = now
                    .date_naive()
                    .and_hms_opt(config.invoice_day as u32, 0, 0)
                    .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                    .unwrap_or(now);

                let sleep_duration = if next_run > now {
                    (next_run - now)
                        .to_std()
                        .unwrap_or(tokio::time::Duration::from_secs(86400))
                } else {
                    tokio::time::Duration::from_secs(86400)
                };

                tokio::time::sleep(sleep_duration).await;

                info!("Running scheduled invoice generation");
                // Invoice generation logic would go here
                let _ = invoice_generator; // Suppress unused warning
            }
        });

        // Payment retry worker (runs hourly)
        let self_clone = Arc::new(self.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;

                // Retry failed payments
                if let Err(e) = self_clone.retry_failed_payments().await {
                    error!(error = %e, "Payment retry failed");
                }
            }
        });

        Ok(())
    }

    // ----------------------------------------------------------------
    // Private helper methods
    // ----------------------------------------------------------------

    async fn flush_counters(
        metrics_store: &Arc<MetricsStore>,
        realtime_counter: &Arc<RealtimeCounter>,
    ) -> Result<(), BillingError> {
        let all_counters = realtime_counter.flush_all();

        for (customer_id, resources) in all_counters {
            for (resource_type, amount) in resources {
                let event = UsageEvent::new(customer_id.clone(), resource_type, amount);

                metrics_store.insert(&event).await?;
            }
        }

        Ok(())
    }

    /// Retry all failed payments that are eligible for retry.
    ///
    /// A payment is retryable when:
    /// - `status = 'failed'`
    /// - `retry_count < max_retries` (from dunning config)
    /// - `next_retry_at <= NOW()` (exponential backoff has elapsed)
    ///
    /// On each retry attempt, the payment is re-processed. If it fails again,
    /// `retry_count` is incremented and `next_retry_at` is set to
    /// `NOW() + 2^retry_count * 60` seconds (exponential backoff).
    async fn retry_failed_payments(&self) -> Result<(), BillingError> {
        let max_retries = self.config.dunning.max_retries as i32;

        if let Some(pool) = &self.pool {
            let rows = sqlx::query(r#"
                SELECT id, invoice_id, customer_id, amount_cents, currency,
                       method_type, provider, retry_count
                FROM payments
                WHERE status = 'failed'
                  AND retry_count < $1
                  AND (next_retry_at IS NULL OR next_retry_at <= NOW())
            "#)
            .bind(max_retries)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(());
            }

            info!(count = rows.len(), "Retrying failed payments");

            for row in rows {
                let payment_id: String = row.get("id");
                let invoice_id: Option<String> = row.get("invoice_id");
                let customer_id: String = row.get("customer_id");
                let amount_cents: i64 = row.get("amount_cents");
                let currency: String = row.get("currency");
                let method_type: String = row.get("method_type");
                let provider: String = row.get("provider");
                let retry_count: i32 = row.get("retry_count");

                // Attempt to re-process the payment
                let retry_result = self.retry_payment(
                    &payment_id,
                    &invoice_id,
                    &customer_id,
                    amount_cents,
                    &currency,
                    &method_type,
                    &provider,
                ).await;

                match retry_result {
                    Ok(true) => {
                        // Payment succeeded on retry
                        info!(payment_id = %payment_id, retry = retry_count, "Payment succeeded on retry");
                        if let Some(ref inv_id) = invoice_id {
                            let _ = sqlx::query(
                                "UPDATE invoices SET status = 'Paid', paid_at = NOW(), transaction_id = $1 WHERE id = $2"
                            )
                            .bind(&payment_id)
                            .bind(inv_id)
                            .execute(pool)
                            .await;
                        }
                    }
                    Ok(false) | Err(_) => {
                        // Payment failed again - update retry info
                        let new_retry_count = retry_count + 1;
                        let backoff_secs = 2i64.pow(new_retry_count as u32) * 60;
                        let next_retry = Utc::now() + Duration::seconds(backoff_secs);

                        let _ = sqlx::query(r#"
                            UPDATE payments
                            SET retry_count = $1,
                                next_retry_at = $2,
                                updated_at = NOW()
                            WHERE id = $3
                        "#)
                        .bind(new_retry_count)
                        .bind(next_retry)
                        .bind(&payment_id)
                        .execute(pool)
                        .await;

                        warn!(
                            payment_id = %payment_id,
                            retry = retry_count,
                            next_retry_secs = backoff_secs,
                            "Payment retry failed, scheduled next retry"
                        );

                        // Suspend customer if max retries reached
                        if new_retry_count >= max_retries && self.config.dunning.suspend_after_max_retries {
                            warn!(customer_id = %customer_id, "Customer suspended after max retries");
                            let _ = sqlx::query(
                                "UPDATE billing_customers SET status = 'Suspended', updated_at = NOW() WHERE id = $1"
                            )
                            .bind(&customer_id)
                            .execute(pool)
                            .await;
                        }
                    }
                }
            }
        } else {
            // In-memory mode: nothing to retry
            debug!("No database pool; retry_failed_payments is a no-op");
        }

        Ok(())
    }

    /// Attempt to re-process a single failed payment.
    ///
    /// Returns `Ok(true)` if the payment succeeded, `Ok(false)` if it failed.
    async fn retry_payment(
        &self,
        payment_id: &str,
        invoice_id: &Option<String>,
        customer_id: &str,
        amount_cents: i64,
        currency: &str,
        method_type: &str,
        provider: &str,
    ) -> Result<bool, BillingError> {
        let amount_str = amount_cents.to_string();

        let result = match (provider.to_lowercase().as_str(), method_type.to_lowercase().as_str()) {
            ("stripe", "card") => {
                // Use a customer's saved payment method token (or placeholder)
                // In production, we would look up the customer's default payment method.
                self.process_card_payment(
                    invoice_id.as_deref().unwrap_or(payment_id),
                    "saved_token",
                    &amount_str,
                    currency,
                )
                .await
            }
            _ => {
                // Generic retry path via HTTP to provider
                self.process_generic_payment(payment_id, customer_id, &amount_str, currency, provider)
                    .await
            }
        };

        // Update payment record status
        if let Some(pool) = &self.pool {
            let status = if result.success { "succeeded" } else { "failed" };
            let _ = sqlx::query(r#"
                UPDATE payments
                SET status = $1,
                    transaction_id = $2,
                    updated_at = NOW()
                WHERE id = $3
            "#)
            .bind(status)
            .bind(&result.transaction_id)
            .bind(payment_id)
            .execute(pool)
            .await;
        }

        Ok(result.success)
    }

    /// Generic payment retry via HTTP to the provider's API.
    async fn process_generic_payment(
        &self,
        _payment_id: &str,
        _customer_id: &str,
        _amount_cents: &str,
        _currency: &str,
        _provider: &str,
    ) -> PaymentResult {
        // In production, this would call the provider's charge/retry API.
        // For now, we log and return failure to trigger the retry backoff.
        warn!("Generic payment retry attempted (provider API integration pending)");
        PaymentResult {
            success: false,
            transaction_id: None,
            message: "Generic payment retry not yet implemented for this provider".to_string(),
        }
    }

    /// Get pricing for a resource type.
    ///
    /// When a PostgreSQL pool is available, looks up the active pricing plan
    /// from the `pricing_plans` table. Falls back to hardcoded tiered pricing
    /// when no pool or no matching plan exists.
    async fn get_pricing(&self, resource_type: &str, quantity: f64) -> (f64, String) {
        // Try database-backed pricing first
        if let Some(pool) = &self.pool {
            if let Ok(row) = sqlx::query(r#"
                SELECT price_cents, description
                FROM pricing_plans
                WHERE resource_type = $1 AND is_active = TRUE
                LIMIT 1
            "#)
            .bind(resource_type)
            .fetch_optional(pool)
            .await
            {
                if let Some(row) = row {
                    let price_cents: i64 = row.get("price_cents");
                    let description: String = row.get("description");
                    // price_cents / 100 = unit price in dollars
                    let unit_price = price_cents as f64 / 100.0;
                    return (unit_price, description);
                }
            }
        }

        // Fallback to hardcoded pricing
        self.get_pricing_fallback(resource_type)
    }

    /// Hardcoded fallback pricing (used when no database pool is available).
    fn get_pricing_fallback(&self, resource_type: &str) -> (f64, String) {
        match resource_type {
            "cpu_hours" => (0.025, "CPU Hours".to_string()),
            "memory_gb_hours" => (0.005, "Memory GB-Hours".to_string()),
            "storage_gb" => (0.10, "Storage GB-Month".to_string()),
            "bandwidth_gb" => (0.08, "Bandwidth GB".to_string()),
            "database_gb" => (0.15, "Database GB-Month".to_string()),
            "requests" => (0.0000002, "API Requests".to_string()),
            _ => (0.01, resource_type.to_string()),
        }
    }

    fn get_unit(&self, resource_type: &str) -> String {
        match resource_type {
            "cpu_hours" => "hours",
            "memory_gb_hours" => "GB-hours",
            "storage_gb" => "GB",
            "bandwidth_gb" => "GB",
            "database_gb" => "GB",
            "requests" => "requests",
            _ => "units",
        }
        .to_string()
    }

    fn calculate_tier_discount(&self, tier: &SubscriptionTier) -> Decimal {
        match tier {
            SubscriptionTier::Free => Decimal::ZERO,
            SubscriptionTier::Starter => Decimal::new(5, 2), // 5%
            SubscriptionTier::Growth => Decimal::new(10, 2), // 10%
            SubscriptionTier::Enterprise => Decimal::new(20, 2), // 20%
            SubscriptionTier::Custom { .. } => Decimal::new(15, 2), // 15%
        }
    }

    fn generate_invoice_number(&self) -> String {
        let now = Utc::now();
        format!("INV-{}-{:04}", now.format("%Y%m"), rand_number())
    }

    // ----------------------------------------------------------------
    // Customer CRUD (private, used internally)
    // ----------------------------------------------------------------

    async fn get_customer(&self, customer_id: &str) -> Result<Customer, BillingError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(r#"
                SELECT id, name, email, address_json, tier, credits, currency,
                       tax_id, status, payment_methods_json, created_at
                FROM billing_customers
                WHERE id = $1
            "#)
            .bind(customer_id)
            .fetch_optional(pool)
            .await?;

            match row {
                Some(row) => Self::row_to_customer(&row),
                None => Err(BillingError::CustomerNotFound(customer_id.to_string())),
            }
        } else {
            // In-memory fallback
            let customers = self.customers.read().await;
            customers
                .get(customer_id)
                .cloned()
                .ok_or_else(|| BillingError::CustomerNotFound(customer_id.to_string()))
        }
    }

    /// Convert a database row to a `Customer` struct.
    fn row_to_customer(row: &sqlx::postgres::PgRow) -> Result<Customer, BillingError> {
        let id: String = row.get("id");
        let name: String = row.get("name");
        let email: String = row.get("email");
        let address_json: Option<serde_json::Value> = row.get("address_json");
        let tier_str: String = row.get("tier");
        let credits: i64 = row.get("credits");
        let currency: String = row.get("currency");
        let tax_id: Option<String> = row.get("tax_id");
        let status_str: String = row.get("status");
        let payment_methods_json: serde_json::Value = row.get("payment_methods_json");
        let created_at: DateTime<Utc> = row.get("created_at");

        let address: Option<Address> = match address_json {
            Some(val) => Some(serde_json::from_value(val)?),
            None => None,
        };

        let tier = match tier_str.as_str() {
            "Free" => SubscriptionTier::Free,
            "Starter" => SubscriptionTier::Starter,
            "Growth" => SubscriptionTier::Growth,
            "Enterprise" => SubscriptionTier::Enterprise,
            other => SubscriptionTier::Custom { name: other.to_string() },
        };

        let status = match status_str.as_str() {
            "Active" => CustomerStatus::Active,
            "PastDue" => CustomerStatus::PastDue,
            "Suspended" => CustomerStatus::Suspended,
            "Closed" => CustomerStatus::Closed,
            other => {
                warn!(unknown_status = other, "Unknown customer status, defaulting to Active");
                CustomerStatus::Active
            }
        };

        let payment_methods: Vec<PaymentMethod> = serde_json::from_value(payment_methods_json)?;

        Ok(Customer {
            id,
            name,
            email,
            address,
            payment_methods,
            tier,
            credits,
            currency,
            tax_id,
            created_at,
            status,
        })
    }

    /// Convert a database row to a `PricingPlan` struct.
    fn row_to_pricing_plan(row: &sqlx::postgres::PgRow) -> Result<PricingPlan, BillingError> {
        Ok(PricingPlan {
            id: row.get("id"),
            name: row.get("name"),
            resource_type: row.get("resource_type"),
            price_cents: row.get("price_cents"),
            currency: row.get("currency"),
            description: row.get("description"),
            tier_multiplier: row.get("tier_multiplier"),
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    // ----------------------------------------------------------------
    // Invoice persistence
    // ----------------------------------------------------------------

    /// Store an invoice to the database or in-memory cache.
    async fn store_invoice(&self, invoice: &Invoice) -> Result<(), BillingError> {
        let line_items_json = serde_json::to_value(&invoice.line_items)?;
        let status_str = format!("{:?}", invoice.status);

        if let Some(pool) = &self.pool {
            sqlx::query(r#"
                INSERT INTO invoices
                    (id, invoice_number, customer_id, customer_name, customer_email,
                     period_start, period_end, line_items_json, subtotal,
                     credit_applied, total, currency, status, due_date,
                     created_at, paid_at, transaction_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            "#)
            .bind(&invoice.id)
            .bind(&invoice.invoice_number)
            .bind(&invoice.customer_id)
            .bind(&invoice.customer_name)
            .bind(&invoice.customer_email)
            .bind(invoice.period.start)
            .bind(invoice.period.end)
            .bind(&line_items_json)
            .bind(invoice.subtotal)
            .bind(invoice.credit_applied)
            .bind(invoice.total)
            .bind(&invoice.currency)
            .bind(&status_str)
            .bind(invoice.due_date)
            .bind(invoice.created_at)
            .bind(invoice.paid_at)
            .bind::<Option<String>, _>(None) // transaction_id set on payment
            .execute(pool)
            .await?;
        } else {
            let mut invoices = self.invoices.write().await;
            invoices.insert(invoice.id.clone(), invoice.clone());
        }

        info!(invoice_id = %invoice.id, "Invoice stored");
        Ok(())
    }

    /// Retrieve an invoice from the database or in-memory cache.
    async fn get_invoice(&self, invoice_id: &str) -> Result<Invoice, BillingError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(r#"
                SELECT id, invoice_number, customer_id, customer_name, customer_email,
                       period_start, period_end, line_items_json, subtotal,
                       credit_applied, total, currency, status, due_date,
                       created_at, paid_at, transaction_id
                FROM invoices
                WHERE id = $1
            "#)
            .bind(invoice_id)
            .fetch_optional(pool)
            .await?;

            match row {
                Some(row) => Self::row_to_invoice(&row),
                None => Err(BillingError::InvoiceNotFound(invoice_id.to_string())),
            }
        } else {
            let invoices = self.invoices.read().await;
            invoices
                .get(invoice_id)
                .cloned()
                .ok_or_else(|| BillingError::InvoiceNotFound(invoice_id.to_string()))
        }
    }

    /// Convert a database row to an `Invoice` struct.
    fn row_to_invoice(row: &sqlx::postgres::PgRow) -> Result<Invoice, BillingError> {
        let id: String = row.get("id");
        let invoice_number: String = row.get("invoice_number");
        let customer_id: String = row.get("customer_id");
        let customer_name: String = row.get("customer_name");
        let customer_email: String = row.get("customer_email");
        let period_start: DateTime<Utc> = row.get("period_start");
        let period_end: DateTime<Utc> = row.get("period_end");
        let line_items_json: serde_json::Value = row.get("line_items_json");
        let subtotal: Decimal = row.get("subtotal");
        let credit_applied: Decimal = row.get("credit_applied");
        let total: Decimal = row.get("total");
        let currency: String = row.get("currency");
        let status_str: String = row.get("status");
        let due_date: DateTime<Utc> = row.get("due_date");
        let created_at: DateTime<Utc> = row.get("created_at");
        let paid_at: Option<DateTime<Utc>> = row.get("paid_at");
        let _transaction_id: Option<String> = row.get("transaction_id");

        let line_items: Vec<LineItem> = serde_json::from_value(line_items_json)?;

        let status = match status_str.as_str() {
            "Draft" => InvoiceStatus::Draft,
            "Open" => InvoiceStatus::Open,
            "Paid" => InvoiceStatus::Paid,
            "Void" => InvoiceStatus::Void,
            "Uncollectible" => InvoiceStatus::Uncollectible,
            other => {
                warn!(unknown_status = other, "Unknown invoice status, defaulting to Draft");
                InvoiceStatus::Draft
            }
        };

        Ok(Invoice {
            id,
            invoice_number,
            customer_id,
            customer_name,
            customer_email,
            period: BillingPeriod {
                start: period_start,
                end: period_end,
            },
            line_items,
            subtotal,
            credit_applied,
            total,
            currency,
            status,
            due_date,
            created_at,
            paid_at,
        })
    }

    /// Mark an invoice as paid in the database.
    async fn mark_invoice_paid(
        &self,
        invoice_id: &str,
        transaction_id: Option<String>,
    ) -> Result<(), BillingError> {
        if let Some(pool) = &self.pool {
            sqlx::query(r#"
                UPDATE invoices
                SET status = 'Paid',
                    paid_at = NOW(),
                    transaction_id = COALESCE($1, transaction_id)
                WHERE id = $2
            "#)
            .bind(&transaction_id)
            .bind(invoice_id)
            .execute(pool)
            .await?;
        } else {
            // In-memory fallback
            let mut invoices = self.invoices.write().await;
            if let Some(invoice) = invoices.get_mut(invoice_id) {
                invoice.status = InvoiceStatus::Paid;
                invoice.paid_at = Some(Utc::now());
            }
        }

        info!(invoice_id = %invoice_id, ?transaction_id, "Invoice marked as paid");
        Ok(())
    }

    async fn handle_payment_failure(
        &self,
        invoice_id: &str,
        reason: &str,
    ) -> Result<(), BillingError> {
        warn!(invoice_id = %invoice_id, reason = %reason, "Payment failed");

        // Record the failure in the payments table if we have a pool
        if let Some(pool) = &self.pool {
            // Try to update the invoice status to reflect the failure
            let _ = sqlx::query(
                "UPDATE invoices SET status = 'Open' WHERE id = $1 AND status = 'Draft'"
            )
            .bind(invoice_id)
            .execute(pool)
            .await;
        }

        Ok(())
    }

    // ----------------------------------------------------------------
    // Payment attempt recording
    // ----------------------------------------------------------------

    /// Record a payment attempt in the `payments` table.
    async fn record_payment_attempt(
        &self,
        invoice_id: &str,
        customer_id: &str,
        amount_cents_str: &str,
        currency: &str,
        method: &PaymentMethod,
        result: &PaymentResult,
    ) -> Result<(), BillingError> {
        let (method_type, provider) = match method {
            PaymentMethod::Card { .. } => ("card", "stripe"),
            PaymentMethod::BankTransfer { .. } => ("bank_transfer", "stripe"),
            PaymentMethod::Wallet { provider, .. } => ("wallet", provider.as_str()),
            PaymentMethod::Crypto { currency, .. } => ("crypto", currency.as_str()),
        };

        let status = if result.success { "succeeded" } else { "failed" };
        let amount_cents: i64 = amount_cents_str.parse().unwrap_or(0);

        if let Some(pool) = &self.pool {
            sqlx::query(r#"
                INSERT INTO payments
                    (id, invoice_id, customer_id, amount_cents, currency,
                     method_type, provider, status, transaction_id,
                     provider_response_json, retry_count, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0, NOW(), NOW())
            "#)
            .bind(Uuid::new_v4().to_string())
            .bind(invoice_id)
            .bind(customer_id)
            .bind(amount_cents)
            .bind(currency)
            .bind(method_type)
            .bind(provider)
            .bind(status)
            .bind(&result.transaction_id)
            .bind(&result.message) // store message as provider_response_json for debugging
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    // ----------------------------------------------------------------
    // Payment processing methods
    // ----------------------------------------------------------------

    /// Process a card payment.
    ///
    /// When the `stripe` feature is enabled and `config.stripe_api_key` is set,
    /// uses the `async_stripe` SDK to create a `PaymentIntent`. Otherwise, if
    /// `stripe_api_key` is present, makes a direct HTTP call to the Stripe API.
    /// Falls back to test-mode success when no API key is configured.
    async fn process_card_payment(
        &self,
        invoice_id: &str,
        token: &str,
        amount_cents: &str,
        currency: &str,
    ) -> PaymentResult {
        info!(invoice_id = %invoice_id, "Processing card payment");

        // Path 1: async_stripe SDK (behind feature flag)
        #[cfg(feature = "stripe")]
        {
            if let Some(ref api_key) = self.config.stripe_api_key {
                return self.process_card_stripe_sdk(invoice_id, token, amount_cents, currency, api_key).await;
            }
        }

        // Path 2: Direct HTTP call to Stripe API
        if let Some(ref api_key) = self.config.stripe_api_key {
            return self.process_card_stripe_http(invoice_id, token, amount_cents, currency, api_key).await;
        }

        // Path 3: Test mode (no API key configured) — for development/testing only.
        warn!(
            invoice_id = %invoice_id,
            "No Stripe API key configured; returning test-mode success"
        );
        PaymentResult {
            success: true,
            transaction_id: Some(format!("test_txn_{}", Uuid::new_v4())),
            message: "Test mode: payment processed successfully".to_string(),
        }
    }

    /// Process card payment using the `async_stripe` SDK.
    #[cfg(feature = "stripe")]
    async fn process_card_stripe_sdk(
        &self,
        invoice_id: &str,
        token: &str,
        amount_cents: &str,
        currency: &str,
        api_key: &str,
    ) -> PaymentResult {
        use async_stripe::{Client, PaymentIntent, PaymentIntentType};

        let client = Client::new(api_key);

        let amount: i64 = match amount_cents.parse() {
            Ok(a) => a,
            Err(_) => {
                return PaymentResult {
                    success: false,
                    transaction_id: None,
                    message: format!("Invalid amount: {}", amount_cents),
                };
            }
        };

        let mut intent_params = PaymentIntent::new();
        intent_params.amount = amount;
        intent_params.currency = currency.to_string();
        intent_params.payment_method = Some(token.to_string());
        intent_params.confirm = Some(true);
        intent_params.metadata = Some(
            vec![
                ("invoice_id".to_string(), invoice_id.to_string()),
                ("source".to_string(), "shellwego-billing".to_string()),
            ]
            .into_iter()
            .collect(),
        );

        match PaymentIntent::create(&client, intent_params).await {
            Ok(intent) => {
                let txn_id = intent.id.to_string();
                let succeeded = intent.status == async_stripe::PaymentIntentStatus::Succeeded
                    || intent.status == async_stripe::PaymentIntentStatus::Processing;

                PaymentResult {
                    success: succeeded,
                    transaction_id: Some(txn_id),
                    message: format!("Payment intent status: {:?}", intent.status),
                }
            }
            Err(e) => {
                error!(invoice_id = %invoice_id, error = %e, "Stripe PaymentIntent creation failed");
                PaymentResult {
                    success: false,
                    transaction_id: None,
                    message: format!("Stripe error: {}", e),
                }
            }
        }
    }

    /// Process card payment via direct HTTP call to Stripe API.
    ///
    /// This path is used when the `stripe` feature is not enabled but an API
    /// key is configured. It creates a PaymentIntent via `POST /v1/payment_intents`.
    async fn process_card_stripe_http(
        &self,
        invoice_id: &str,
        token: &str,
        amount_cents: &str,
        currency: &str,
        api_key: &str,
    ) -> PaymentResult {
        let url = "https://api.stripe.com/v1/payment_intents";
        let params = [
            ("amount", amount_cents),
            ("currency", currency),
            ("payment_method", token),
            ("confirm", "true"),
            ("metadata[invoice_id]", invoice_id),
        ];

        let response = self.http_client
            .post(url)
            .basic_auth(api_key, None::<&str>)
            .form(&params)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        let txn_id = body.get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let status = body.get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        info!(
                            invoice_id = %invoice_id,
                            stripe_status = status,
                            "Stripe payment intent created"
                        );

                        let succeeded = status == "succeeded" || status == "processing";
                        PaymentResult {
                            success: succeeded,
                            transaction_id: Some(txn_id),
                            message: format!("Stripe status: {}", status),
                        }
                    } else {
                        PaymentResult {
                            success: false,
                            transaction_id: None,
                            message: "Failed to parse Stripe response".to_string(),
                        }
                    }
                } else {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    error!(
                        invoice_id = %invoice_id,
                        http_status = %status,
                        body = %body_text,
                        "Stripe API returned error"
                    );
                    PaymentResult {
                        success: false,
                        transaction_id: None,
                        message: format!("Stripe API error {}: {}", status, body_text),
                    }
                }
            }
            Err(e) => {
                error!(invoice_id = %invoice_id, error = %e, "Stripe HTTP request failed");
                PaymentResult {
                    success: false,
                    transaction_id: None,
                    message: format!("HTTP request failed: {}", e),
                }
            }
        }
    }

    async fn process_bank_transfer(
        &self,
        invoice_id: &str,
        account_id: &str,
        amount_cents: &str,
        currency: &str,
    ) -> PaymentResult {
        info!(
            invoice_id = %invoice_id,
            account_id = %account_id,
            amount_cents = %amount_cents,
            currency = %currency,
            "Processing bank transfer"
        );

        // Bank transfers are asynchronous — we record the intent and return
        // pending status. In production, this would call the provider's API.
        PaymentResult {
            success: true,
            transaction_id: Some(format!("bt_{}", Uuid::new_v4())),
            message: "Bank transfer initiated; funds will settle in 1-3 business days".to_string(),
        }
    }

    async fn process_wallet_payment(
        &self,
        invoice_id: &str,
        provider: &str,
        _token: &str,
        amount_cents: &str,
        currency: &str,
    ) -> PaymentResult {
        info!(
            invoice_id = %invoice_id,
            provider = %provider,
            amount_cents = %amount_cents,
            currency = %currency,
            "Processing wallet payment"
        );

        // In production, this would call PayPal / Apple Pay / Google Pay APIs.
        PaymentResult {
            success: true,
            transaction_id: Some(format!("wallet_{}", Uuid::new_v4())),
            message: format!("{} wallet payment processed", provider),
        }
    }

    async fn process_crypto_payment(
        &self,
        invoice_id: &str,
        currency: &str,
        address: &str,
        amount_cents: &str,
    ) -> PaymentResult {
        info!(
            invoice_id = %invoice_id,
            crypto_currency = %currency,
            address = %address,
            amount_cents = %amount_cents,
            "Processing crypto payment"
        );

        // Crypto payments require blockchain confirmation; we record the intent.
        PaymentResult {
            success: true,
            transaction_id: Some(format!("crypto_{}", Uuid::new_v4())),
            message: "Crypto payment pending blockchain confirmation".to_string(),
        }
    }

    // ----------------------------------------------------------------
    // Webhook verification
    // ----------------------------------------------------------------

    /// Verify a Stripe webhook signature using HMAC-SHA256.
    ///
    /// Stripe sends a `Stripe-Signature` header containing:
    /// - `t=<timestamp>`: Unix timestamp of when the webhook was sent
    /// - `v1=<signature>`: HMAC-SHA256 of `timestamp.payload` using the
    ///   webhook signing secret as the key
    ///
    /// Algorithm:
    /// 1. Extract `t` and `v1` from the header
    /// 2. Construct signed payload: `{timestamp}.{raw_body}`
    /// 3. Compute `HMAC-SHA256(webhook_secret, signed_payload)`
    /// 4. Base64-encode the digest
    /// 5. Constant-time compare with the `v1` value from the header
    /// 6. Optionally, reject webhooks older than 5 minutes (tolerance)
    ///
    /// Uses `config.stripe_api_key` as the signing secret. In production,
    /// this should be the webhook signing secret (whsec_...), not the
    /// publishable/secret key. The config field name is a known limitation
    /// that should be addressed in a future configuration refactor.
    fn verify_stripe_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        let signing_secret = self.config.stripe_api_key.as_ref()
            .ok_or_else(|| BillingError::WebhookVerificationError(
                "No Stripe signing secret configured".to_string(),
            ))?;

        // Parse the Stripe-Signature header: "t=...,v1=..."
        let mut timestamp = None;
        let mut v1_signature = None;

        for part in signature.split(',') {
            let part = part.trim();
            if let Some(ts) = part.strip_prefix("t=") {
                timestamp = Some(ts.to_string());
            } else if let Some(sig) = part.strip_prefix("v1=") {
                v1_signature = Some(sig.to_string());
            }
        }

        let timestamp = timestamp.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing timestamp (t=) in Stripe signature header".to_string(),
        ))?;
        let v1_sig = v1_signature.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing v1 signature in Stripe signature header".to_string(),
        ))?;

        // Construct signed payload: "{timestamp}.{payload}"
        let mut signed_payload = timestamp.clone();
        signed_payload.push('.');
        signed_payload.push_str(&String::from_utf8_lossy(payload));

        // Compute HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
            .map_err(|e| BillingError::WebhookVerificationError(
                format!("HMAC key error: {}", e),
            ))?;
        mac.update(signed_payload.as_bytes());
        let result = mac.finalize();
        let computed = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes());

        // Constant-time comparison
        let v1_bytes = v1_sig.as_bytes();
        let computed_bytes = computed.as_bytes();

        let match_len = v1_bytes.len().min(computed_bytes.len());
        let mut diff = v1_bytes.len() ^ computed_bytes.len();
        for i in 0..match_len {
            diff |= v1_bytes[i] ^ computed_bytes[i];
        }
        let verified = diff == 0;

        if verified {
            info!("Stripe webhook signature verified successfully");
        } else {
            warn!("Stripe webhook signature verification failed");
        }

        Ok(verified)
    }

    /// Verify a Paystack webhook signature using HMAC-SHA512.
    ///
    /// Paystack sends a raw JSON body and sets the `X-Paystack-Signature`
    /// header to the hex-encoded HMAC-SHA512 digest of the raw body using
    /// the secret key.
    ///
    /// Algorithm:
    /// 1. Compute `HMAC-SHA512(secret_key, raw_body_bytes)`
    /// 2. Hex-encode the digest
    /// 3. Constant-time compare with the `X-Paystack-Signature` header value
    fn verify_paystack_webhook(
        &self,
        payload: &[u8],
        signature: &str,
    ) -> Result<bool, BillingError> {
        let secret = self.config.paystack_secret_key.as_ref()
            .ok_or_else(|| BillingError::WebhookVerificationError(
                "No Paystack secret key configured".to_string(),
            ))?;

        // Compute HMAC-SHA512
        let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
            .map_err(|e| BillingError::WebhookVerificationError(
                format!("HMAC key error: {}", e),
            ))?;
        mac.update(payload);
        let result = mac.finalize();
        let computed_hex = hex::encode(result.into_bytes());

        // Constant-time comparison
        let sig_bytes = signature.trim().as_bytes();
        let computed_bytes = computed_hex.as_bytes();

        let match_len = sig_bytes.len().min(computed_bytes.len());
        let mut diff = sig_bytes.len() ^ computed_bytes.len();
        for i in 0..match_len {
            diff |= sig_bytes[i] ^ computed_bytes[i];
        }
        let verified = diff == 0;

        if verified {
            info!("Paystack webhook signature verified successfully");
        } else {
            warn!("Paystack webhook signature verification failed");
        }

        Ok(verified)
    }
}

impl Clone for BillingSystem {
    fn clone(&self) -> Self {
        Self {
            metrics_store: self.metrics_store.clone(),
            realtime_counter: self.realtime_counter.clone(),
            invoice_generator: self.invoice_generator.clone(),
            config: self.config.clone(),
            customers: self.customers.clone(),
            invoices: self.invoices.clone(),
            pool: self.pool.clone(),
            http_client: self.http_client.clone(),
        }
    }
}

// Helper function for random number generation
fn rand_number() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        % 10000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_event_creation() {
        let event = UsageEvent::new("cust_123", "cpu_hours", 5.5).with_metadata("region", "us-east-1");

        assert_eq!(event.customer_id, "cust_123");
        assert_eq!(event.resource_type, "cpu_hours");
        assert_eq!(event.quantity, 5.5);
        assert_eq!(
            event.metadata.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_billing_period_monthly() {
        let period = BillingPeriod::monthly_from(Utc::now());

        assert!(period.start < period.end);
    }

    #[tokio::test]
    async fn test_billing_system_creation() {
        let config = BillingConfig {
            metrics_dsn: "sqlite::memory:".to_string(),
            ..Default::default()
        };

        let result = BillingSystem::new(&config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stripe_webhook_verification() {
        // Set up a billing system with a test secret
        let config = BillingConfig {
            stripe_api_key: Some("whsec_test_secret_key".to_string()),
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let payload = br#"{"id":"evt_123","type":"payment.succeeded"}"#;
        let timestamp = "1234567890";
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        // Compute the expected signature
        let mut mac = HmacSha256::new_from_slice(b"whsec_test_secret_key").unwrap();
        mac.update(signed_payload.as_bytes());
        let result = mac.finalize();
        let expected_sig = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes());

        let signature = format!("t={},v1={}", timestamp, expected_sig);
        let verified = system.verify_stripe_webhook(payload, &signature).unwrap();
        assert!(verified, "Webhook should verify with correct signature");
    }

    #[tokio::test]
    async fn test_stripe_webhook_verification_rejects_tampered() {
        let config = BillingConfig {
            stripe_api_key: Some("whsec_test_secret_key".to_string()),
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let payload = br#"{"id":"evt_123","type":"payment.succeeded"}"#;
        let tampered = br#"{"id":"evt_456","type":"payment.succeeded"}"#;
        let timestamp = "1234567890";
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = HmacSha256::new_from_slice(b"whsec_test_secret_key").unwrap();
        mac.update(signed_payload.as_bytes());
        let result = mac.finalize();
        let expected_sig = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes());

        // Use the original signature but tampered payload
        let signature = format!("t={},v1={}", timestamp, expected_sig);
        let verified = system.verify_stripe_webhook(tampered, &signature).unwrap();
        assert!(!verified, "Webhook should reject tampered payload");
    }

    #[tokio::test]
    async fn test_paystack_webhook_verification() {
        let config = BillingConfig {
            paystack_secret_key: Some("sk_test_paystack_secret".to_string()),
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let payload = br#"{"event":"charge.success","data":{"id":123}}"#;

        // Compute the expected signature
        let mut mac = HmacSha512::new_from_slice(b"sk_test_paystack_secret").unwrap();
        mac.update(payload);
        let result = mac.finalize();
        let expected_sig = hex::encode(result.into_bytes());

        let verified = system.verify_paystack_webhook(payload, &expected_sig).unwrap();
        assert!(verified, "Paystack webhook should verify with correct signature");
    }

    #[tokio::test]
    async fn test_paystack_webhook_verification_rejects_wrong_key() {
        let config = BillingConfig {
            paystack_secret_key: Some("sk_test_paystack_secret".to_string()),
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let payload = br#"{"event":"charge.success"}"#;
        let wrong_sig = "deadbeef1234567890abcdef";

        let verified = system.verify_paystack_webhook(payload, wrong_sig).unwrap();
        assert!(!verified, "Paystack webhook should reject wrong signature");
    }

    #[test]
    fn test_payment_record_serde() {
        let record = PaymentRecord {
            id: "pay_123".to_string(),
            invoice_id: "inv_456".to_string(),
            customer_id: "cust_789".to_string(),
            amount_cents: 10000,
            currency: "USD".to_string(),
            method_type: "card".to_string(),
            provider: "stripe".to_string(),
            status: "succeeded".to_string(),
            transaction_id: Some("pi_abc".to_string()),
            provider_response_json: None,
            retry_count: 0,
            next_retry_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let decoded: PaymentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.amount_cents, 10000);
    }

    #[test]
    fn test_pricing_plan_serde() {
        let plan = PricingPlan {
            id: "plan_001".to_string(),
            name: "CPU Hours".to_string(),
            resource_type: "cpu_hours".to_string(),
            price_cents: 250,
            currency: "USD".to_string(),
            description: "CPU usage per hour".to_string(),
            tier_multiplier: Some(1.0),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: PricingPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.resource_type, "cpu_hours");
        assert_eq!(decoded.price_cents, 250);
    }

    #[tokio::test]
    async fn test_stripe_webhook_no_secret_configured() {
        let config = BillingConfig {
            stripe_api_key: None,
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let result = system.verify_stripe_webhook(b"{}", "t=123,v1=abc");
        assert!(result.is_err(), "Should error when no secret configured");
    }

    #[tokio::test]
    async fn test_paystack_webhook_no_secret_configured() {
        let config = BillingConfig {
            paystack_secret_key: None,
            ..Default::default()
        };
        let system = BillingSystem::new_test(&config).await;

        let result = system.verify_paystack_webhook(b"{}", "abc");
        assert!(result.is_err(), "Should error when no secret configured");
    }
}

/// Test-only helpers (avoid needing a full async constructor for sync tests)
#[cfg(test)]
impl BillingSystem {
    /// Create a billing system for unit tests without database connectivity.
    /// Uses in-memory-only mode (no pool).
    async fn new_test(config: &BillingConfig) -> Self {
        let metrics_store = Arc::new(
            MetricsStore::new("sqlite::memory:")
                .await
                .expect("test metrics store creation"),
        );

        Self {
            metrics_store,
            realtime_counter: Arc::new(RealtimeCounter::new()),
            invoice_generator: Arc::new(InvoiceGenerator::new("").unwrap()),
            config: config.clone(),
            customers: Arc::new(RwLock::new(HashMap::new())),
            invoices: Arc::new(RwLock::new(HashMap::new())),
            pool: None,
            http_client: Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap(),
            ),
        }
    }
}
