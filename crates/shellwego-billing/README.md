# ShellWeGo Billing

**Turning infrastructure into income.** The commercial layer of the ShellWeGo platform.

## Overview

This crate provides comprehensive billing, metering, and payment processing capabilities for ShellWeGo deployments. It enables platform operators to monetize their infrastructure through usage-based billing with support for multiple payment providers.

## Features

### Metering (`metering.rs`)
- **High-throughput usage tracking** for CPU, memory, storage, and network resources
- **Time-series storage** with PostgreSQL/TimescaleDB backend
- **Real-time counters** for dashboard displays and rate limiting
- **Multi-granularity aggregation** (raw, minute, hour, day, month)
- **Automatic data retention** and cleanup

### Invoicing (`invoices.rs`)
- **PDF invoice generation** from Tera templates
- **Email delivery** integration ready
- **Proration calculations** for partial billing periods
- **Multi-currency support** (USD, EUR, GBP, NGN, KES, INR, etc.)
- **Professional HTML templates** with custom branding

### Billing System (`lib.rs`)
- **Usage-based billing** with tiered pricing
- **Subscription tier discounts** (Free, Starter, Growth, Enterprise)
- **Payment processing** via multiple providers:
  - Credit/debit cards (Stripe)
  - Bank transfers
  - Digital wallets
  - Cryptocurrency
- **Webhook handling** for payment notifications
- **Dunning management** for failed payments
- **Background workers** for aggregation and invoicing

## Usage

### Basic Setup

```rust
use shellwego_billing::{BillingSystem, BillingConfig, UsageEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the billing system
    let config = BillingConfig {
        currency: "USD".to_string(),
        metrics_dsn: "postgres://localhost/billing".to_string(),
        stripe_api_key: Some("sk_test_...".to_string()),
        ..Default::default()
    };
    
    // Initialize
    let billing = BillingSystem::new(&config).await?;
    
    // Record usage
    let event = UsageEvent::new("customer_123", "cpu_hours", 5.5)
        .with_metadata("region", "us-east-1");
    billing.record_usage(event).await?;
    
    // Generate invoice
    let period = BillingPeriod::monthly_from(Utc::now());
    let invoice = billing.generate_invoice("customer_123", period).await?;
    
    Ok(())
}
```

### Metering

```rust
use shellwego_billing::{MetricsStore, RealtimeCounter, Granularity};

// Create metrics store
let store = MetricsStore::new("postgres://localhost/metrics").await?;

// Insert usage event
store.insert(&event).await?;

// Query aggregated data
let data = store.query(
    "customer_123",
    "cpu_hours",
    start_time,
    end_time,
    Granularity::Day,
).await?;

// Get current month totals
let totals = store.current_month_total("customer_123").await?;
```

### Invoice Generation

```rust
use shellwego_billing::{InvoiceGenerator, BrandingConfig};

// Create generator with custom branding
let branding = BrandingConfig {
    company_name: "MyCloud".to_string(),
    primary_color: "#00D4AA".to_string(),
    email: "billing@mycloud.com".to_string(),
    ..Default::default()
};

let generator = InvoiceGenerator::with_branding("./templates", branding)?;

// Generate PDF
let pdf = generator.generate_pdf(&invoice).await?;

// Send via email
generator.send_email(&invoice, &pdf, "customer@example.com").await?;
```

## Pricing Structure

Default tiered pricing is included:

| Resource | Low Volume | Medium | High Volume |
|----------|-----------|--------|-------------|
| CPU Hours | $0.025/hr | $0.02/hr | $0.015/hr |
| Memory GB-Hours | $0.005/GB-hr | $0.004/GB-hr | $0.003/GB-hr |
| Storage GB | $0.10/GB-mo | - | - |
| Bandwidth GB | $0.08/GB | - | $0.05/GB |
| Database GB | $0.15/GB-mo | - | - |

Subscription tier discounts:
- Free: 0%
- Starter: 5%
- Growth: 10%
- Enterprise: 20%

## Database Schema

The billing system expects a PostgreSQL database with the following schema (auto-created):

```sql
CREATE TABLE usage_events (
    id BIGSERIAL,
    customer_id VARCHAR(255) NOT NULL,
    resource_type VARCHAR(100) NOT NULL,
    quantity DOUBLE PRECISION NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (id, timestamp)
);
```

For production, TimescaleDB extension is recommended for time-series optimization.

## Feature Flags

- `default` - Core billing functionality
- `pdf` - Enable PDF generation via headless Chrome
- `stripe` - Enable Stripe payment integration

## License

AGPL-3.0-or-later
