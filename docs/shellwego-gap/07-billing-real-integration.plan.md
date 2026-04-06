# Plan 07: Billing & Real Payment Integration

## 1. Title & Overview

**Billing & Real Payment Integration** — The `shellwego-billing` crate sits at ~70% parity with the README billing feature-set and currently fails to compile (4 errors: ownership/lifetime issues and a private method access). All payment processing methods other than Stripe cards are stubs that immediately return success without contacting any external API. Webhook verification works for Stripe (HMAC-SHA256) and Paystack (HMAC-SHA512) but no other providers are supported. There are zero integrations with M-Pesa, GCash, UPI, or MercadoPago. Crypto payment processing is a no-op. Invoice PDF generation falls back to raw HTML bytes when the `pdf` feature flag is not enabled, and the headless-chrome path has never been tested in CI. This plan (a) fixes the 4 build errors, (b) adds a `PaymentProvider` trait with real Stripe, Paystack, M-Pesa, GCash, UPI, and MercadoPago implementations, (c) replaces crypto stubs with a real blockchain-monitoring integration, (d) hardens invoice PDF generation, and (e) adds a `transaction_id` field to the `Invoice` schema type. After execution, the billing crate should compile cleanly and every advertised payment method should be wired to a real (or pluggable mock) provider backend.

## 2. Gap Summary

| # | README / Doc Claim | Actual State | File(s) | Severity |
|---|---|---|---|---|
| A | "Multi-provider payment processing (Stripe, Paystack, etc.)" | Only Stripe card payments have real HTTP calls. `process_bank_transfer`, `process_wallet_payment`, `process_crypto_payment` are unconditional success stubs. `process_generic_payment` is a no-op. | `src/lib.rs:1795-1864, 1180-1196` | **CRITICAL** |
| B | Build fails with 4 errors | Ownership/lifetime issues and a private method access prevent compilation. Crate is currently broken. | `src/lib.rs` (build output) | **CRITICAL** |
| C | "M-Pesa, GCash, UPI, MercadoPago integrations" | Zero code exists for these providers. `BillingConfig` has no config fields for them. `handle_webhook` returns error for unknown providers. | `src/lib.rs:858-867`, `src/billing/config.rs` | **HIGH** |
| D | "Crypto payment processing" | `PaymentMethod::Crypto` variant exists in schema, but `process_crypto_payment` returns a fake success with `format!("crypto_{}", Uuid::new_v4())`. No blockchain confirmation monitoring, no amount conversion, no address validation. | `src/lib.rs:1843-1864` | **HIGH** |
| E | Invoice PDF rendering | `InvoiceGenerator::generate_pdf` exists. Without `pdf` feature, returns raw HTML bytes (not valid PDF). With `pdf` feature, uses `headless_chrome` which requires Chrome binary on the host. Never tested in CI. | `src/invoices.rs:214-259` | **MEDIUM** |
| F | `Invoice` struct missing `transaction_id` | Database schema (`invoices` table) has a `transaction_id` column. `store_invoice` and `mark_invoice_paid` write to it. But the `Invoice` struct in schema (`crates/shellwego-schema/src/billing/invoice.rs`) has no `transaction_id` field — it is read with an underscore prefix (`_transaction_id`) and discarded in `row_to_invoice`. | `src/billing/invoice.rs:49-83`, `src/lib.rs:1464,1480-1499` | **MEDIUM** |
| G | Webhook event parsing is generic | `WebhookEvent` struct uses `event_type: String` and `data: HashMap<String, Value>`. Real Stripe webhooks use `type` and `data.object` with nested structures. Real Paystack webhooks use `event` and `data`. Neither is parsed correctly — parsing will fail on real payloads. | `src/lib.rs:100-104, 876` | **HIGH** |
| H | Stripe webhook uses wrong config field | `verify_stripe_webhook` reads `self.config.stripe_api_key` as the signing secret. The docstring acknowledges this: "should be the webhook signing secret (whsec_...), not the publishable/secret key." | `src/lib.rs:1889-1893` | **MEDIUM** |
| I | Dunning emails are never sent | `DunningConfig` has `email_templates: Vec<String>` field but `retry_failed_payments` only updates DB status and suspends accounts. No email notifications are triggered. | `src/lib.rs:1053-1123` | **LOW** |
| J | No refund support | `PaymentRecord` has a `status` that includes "refunded" in the docstring but there is no `refund_payment`, `process_refund`, or partial refund logic anywhere. | `src/lib.rs:126-127` | **MEDIUM** |
| K | Migration SQL uses SQLite types while in-code schema uses PostgreSQL types | `003_add_billing_tables.sql` uses `TEXT`, `INTEGER`, `REAL`, `datetime('now')`. `ensure_billing_schema()` uses `VARCHAR`, `BIGINT`, `NUMERIC(20,6)`, `TIMESTAMPTZ`, `JSONB`, `NOW()`. These are incompatible — the migration will not work on PostgreSQL. | `migrations/003_add_billing_tables.sql`, `src/lib.rs:349-434` | **MEDIUM** |

## 3. Scope

### Files to Modify

| File | Change |
|---|---|
| `crates/shellwego-billing/src/lib.rs` | Fix 4 build errors; extract `PaymentProvider` trait; add provider dispatch; fix webhook parsing; add `stripe_webhook_secret` config usage; add refund logic; fix `run_workers` ownership |
| `crates/shellwego-billing/src/invoices.rs` | Add real PDF generation with `genpdf` or `printpdf` (replacing `headless_chrome`); fix `generate_invoice_number` accessibility for tests |
| `crates/shellwego-billing/Cargo.toml` | Add `genpdf` or `printpdf` dependency; add `mpesa` / `mercadopago` optional deps; update feature flags |
| `crates/shellwego-schema/src/billing/invoice.rs` | Add `transaction_id: Option<String>` field to `Invoice` struct |
| `crates/shellwego-schema/src/billing/config.rs` | Add `stripe_webhook_secret`, `mpesa_*`, `gcash_*`, `upi_*`, `mercadopago_*` config fields; add `PaymentProviderConfig` |
| `crates/shellwego-schema/src/billing/customer.rs` | Add `RefundMethod` enum or extend `PaymentMethod` for refunds |
| `migrations/003_add_billing_tables.sql` | Rewrite for PostgreSQL compatibility (replace `TEXT/INTEGER/REAL` with proper PostgreSQL types); add `transaction_id` column handling |

### New Files to Create

| File | Purpose |
|---|---|
| `crates/shellwego-billing/src/providers/mod.rs` | Module root for payment provider implementations |
| `crates/shellwego-billing/src/providers/stripe.rs` | Stripe payment provider (PaymentIntent API, webhook parsing, refund) |
| `crates/shellwego-billing/src/providers/paystack.rs` | Paystack payment provider (charge API, webhook parsing, refund) |
| `crates/shellwego-billing/src/providers/mpesa.rs` | M-Pesa (Safaricom Daraja API) payment provider |
| `crates/shellwego-billing/src/providers/gcash.rs` | GCash (via PayMongo or similar gateway) payment provider |
| `crates/shellwego-billing/src/providers/upi.rs` | UPI (Razorpay or similar gateway) payment provider |
| `crates/shellwego-billing/src/providers/mercadopago.rs` | Mercado Pago payment provider |
| `crates/shellwego-billing/src/providers/crypto.rs` | Crypto payment provider (address generation, confirmation monitoring via mempool.space/blockchain.com APIs) |
| `crates/shellwego-billing/src/providers/mock.rs` | Mock provider for testing (returns configurable success/failure) |

## 4. Prerequisites

### P1. Fix 4 Build Errors (CRITICAL — must be first)

**Root cause analysis** — Without a Rust toolchain in this environment, the 4 errors are diagnosed from code review:

**Error 1 & 2: Ownership / lifetime in `run_workers`**
At line 975: `let self_clone = Arc::new(self.clone());` — The manual `Clone` impl (line 1999-2012) clones `self.pool: Option<PgPool>`. While `PgPool` implements `Clone`, this creates a second pool handle. The closure in the spawned task captures `self_clone` by move, but the `retry_failed_payments(&self)` method borrows `self`. Since `self_clone` is `Arc<BillingSystem>`, this should work. The likely actual error is that `retry_failed_payments` accesses `self.config` and `self.pool` which require the full `&self` borrow, and `Arc::new(self.clone())` on a non-`Clone` type would fail. **Fix:** Ensure `BillingSystem` implements `Clone` (already does manually). If the errors are about `self.pool` being moved into the spawned task, wrap in `Arc`.

**Error 3: Private method `generate_invoice_number` in `InvoiceGenerator`**
At line 447 of `invoices.rs`, `generate_invoice_number` is `fn` (private). Tests at line 1060 call `generator.generate_invoice_number()`. **Fix:** Make the method `pub fn` or `pub(crate) fn`.

**Error 4: Lifetime/ownership in `flush_counters` (static method)**
`Self::flush_counters(&metrics_store, &realtime_counter)` is a static call within an `async` block spawned by `tokio::spawn`. The `Arc` clones should be `Send + Sync`, but `DashMap` is `Send + Sync`. The likely issue is that the spawned task's future captures `&self` by reference, but the `tokio::spawn` requires `'static`. **Fix:** Clone the `Arc` values before entering the spawn closure (already done at lines 932-933 for the first worker — verify the second and third workers do the same).

**Verification:** After fixes, run `cargo check -p shellwego-billing` — must produce 0 errors.

### P2. Add `transaction_id` to `Invoice` struct

File: `crates/shellwego-schema/src/billing/invoice.rs`

Add field after `paid_at`:
```rust
/// Transaction ID from payment provider (set when paid)
pub transaction_id: Option<String>,
```

Update all construction sites:
- `BillingSystem::generate_invoice` in `lib.rs` — set to `None`
- `InvoiceGenerator::generate_from_usage` in `invoices.rs` — set to `None`
- `BillingSystem::row_to_invoice` in `lib.rs` — read `_transaction_id` and assign to `transaction_id` (remove underscore prefix)
- All test `Invoice` structs across `lib.rs` tests and `invoices.rs` tests — add `transaction_id: None`
- All test `Invoice` structs in `invoice.rs` schema tests — add `transaction_id: None`

### P3. Fix Migration SQL for PostgreSQL

File: `migrations/003_add_billing_tables.sql`

Replace SQLite-specific types:
| SQLite | PostgreSQL |
|---|---|
| `TEXT PRIMARY KEY` | `VARCHAR(255) PRIMARY KEY` |
| `TEXT NOT NULL` | `VARCHAR(255) NOT NULL` |
| `TEXT DEFAULT NULL` | `TEXT DEFAULT NULL` (OK) |
| `INTEGER NOT NULL DEFAULT 0` | `BIGINT NOT NULL DEFAULT 0` |
| `REAL NOT NULL DEFAULT 0` | `NUMERIC(20,6) NOT NULL DEFAULT 0` |
| `datetime('now')` | `NOW()` |
| `'[]'` (JSONB) | `'[]'::jsonb` |
| `INTEGER NOT NULL DEFAULT 1` (boolean) | `BOOLEAN NOT NULL DEFAULT TRUE` |

Add a `transaction_id` column to `invoices` table:
```sql
transaction_id VARCHAR(255) DEFAULT NULL
```

### P4. Add `genpdf` dependency for PDF generation

File: `crates/shellwego-billing/Cargo.toml`

Replace `headless_chrome` with a pure-Rust PDF library:
```toml
# PDF generation (pure Rust, no Chrome dependency)
genpdf = { version = "0.2", optional = true }

# Replace: headless_chrome = { version = "1.0", optional = true }
```

Update feature flags:
```toml
[features]
default = []
pdf = ["dep:genpdf"]
stripe = ["dep:async-stripe"]
mpesa = []
paystack-provider = []
```

### P5. Add provider configuration to `BillingConfig`

File: `crates/shellwego-schema/src/billing/config.rs`

Add fields to `BillingConfig`:
```rust
/// Stripe webhook signing secret (whsec_...)
pub stripe_webhook_secret: Option<String>,

/// M-Pesa (Safaricom Daraja) configuration
pub mpesa_config: Option<MpesaConfig>,

/// GCash configuration (via PayMongo)
pub gcash_config: Option<GcashConfig>,

/// UPI configuration (via Razorpay)
pub upi_config: Option<UpiConfig>,

/// Mercado Pago configuration
pub mercadopago_config: Option<MercadoPagoConfig>,

/// Crypto payment configuration
pub crypto_config: Option<CryptoConfig>,
```

Add new config structs:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpesaConfig {
    pub consumer_key: String,
    pub consumer_secret: String,
    pub passkey: String,
    pub business_short_code: String,
    pub environment: MpesaEnvironment,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MpesaEnvironment {
    Sandbox,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcashConfig {
    pub paymongo_public_key: String,
    pub paymongo_secret_key: String,
    pub webhook_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpiConfig {
    pub razorpay_key_id: String,
    pub razorpay_key_secret: String,
    pub webhook_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MercadoPagoConfig {
    pub access_token: String,
    pub webhook_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoConfig {
    /// Blockchain exploration API URL (e.g., "https://mempool.space/api")
    pub mempool_api_url: String,
    /// Number of required confirmations
    pub required_confirmations: u32,
    /// Conversion rate provider URL
    pub rate_api_url: String,
    /// Supported currencies and their network identifiers
    pub supported_currencies: Vec<CryptoCurrencyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoCurrencyConfig {
    pub code: String,         // "BTC", "ETH", "USDC"
    pub network: String,      // "bitcoin", "ethereum", "polygon"
    pub decimals: u8,         // 8 for BTC, 18 for ETH, 6 for USDC
    pub confirmation_timeout_minutes: u32,
}
```

## 5. Detailed Implementation Steps

### Phase 0: Build Fix & Schema Alignment

**Step 0.1 — Fix `generate_invoice_number` visibility**

File: `crates/shellwego-billing/src/invoices.rs`, line 447

Change:
```rust
fn generate_invoice_number(&self) -> String {
```
To:
```rust
pub fn generate_invoice_number(&self) -> String {
```

**Step 0.2 — Fix `run_workers` ownership**

File: `crates/shellwego-billing/src/lib.rs`, lines 928-988

The three spawned tasks need `'static` futures. Verify each closure captures `Arc` clones, not references:

Worker 1 (usage flush) — already correct (lines 932-943 clone `Arc` values).

Worker 2 (invoice scheduler) — already correct (lines 946-971 clone `Arc` values).

Worker 3 (payment retry) — problematic at line 975:
```rust
let self_clone = Arc::new(self.clone());
```
This clones the entire `BillingSystem`, then wraps in `Arc`. Since `BillingSystem` already implements `Clone`, this is valid but wasteful. The issue may be that `retry_failed_payments` is called on `&self_clone` but `self_clone` is `Arc<BillingSystem>`, so it auto-derefs. **Fix:** If this still errors, wrap `self` in `Arc` at the struct level or use `Arc::clone` from a stored `Arc<Self>`:
```rust
tokio::spawn(async move {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        if let Err(e) = self_clone.retry_failed_payments().await {
            error!(error = %e, "Payment retry failed");
        }
    }
});
```

**Step 0.3 — Add `transaction_id` to `Invoice`**

File: `crates/shellwego-schema/src/billing/invoice.rs`

Add after `paid_at`:
```rust
/// Transaction ID from payment provider (set when paid)
pub transaction_id: Option<String>,
```

Update every `Invoice { ... }` construction to include `transaction_id: None`. Files affected:
- `crates/shellwego-schema/src/billing/invoice.rs` tests (2 instances)
- `crates/shellwego-billing/src/lib.rs` `generate_invoice` method
- `crates/shellwego-billing/src/invoices.rs` `generate_from_usage` method
- `crates/shellwego-billing/src/invoices.rs` tests (4 instances)
- `crates/shellwego-billing/src/lib.rs` tests (0 instances — no direct Invoice construction)

File: `crates/shellwego-billing/src/lib.rs`

In `row_to_invoice` (line 1464), change:
```rust
let _transaction_id: Option<String> = row.get("transaction_id");
```
To:
```rust
let transaction_id: Option<String> = row.get("transaction_id");
```

And in the `Ok(Invoice { ... })` block (line 1498), add before the closing brace:
```rust
transaction_id,
```

In `generate_invoice` (line 758-774), add to Invoice construction:
```rust
transaction_id: None,
```

In `mark_invoice_paid`, also update the in-memory fallback (line 1522-1526):
```rust
if let Some(invoice) = invoices.get_mut(invoice_id) {
    invoice.status = InvoiceStatus::Paid;
    invoice.paid_at = Some(Utc::now());
    invoice.transaction_id = transaction_id; // <-- add this
}
```

**Step 0.4 — Fix Stripe webhook secret config**

File: `crates/shellwego-billing/src/lib.rs`, `verify_stripe_webhook` (line 1889)

Change:
```rust
let signing_secret = self.config.stripe_api_key.as_ref()
```
To:
```rust
let signing_secret = self.config.stripe_webhook_secret.as_ref()
    .or(self.config.stripe_api_key.as_ref())
```

This prefers the dedicated webhook secret, falls back to API key for backwards compatibility.

**Step 0.5 — Fix migration SQL**

File: `migrations/003_add_billing_tables.sql`

Rewrite entirely with PostgreSQL types. Add `transaction_id` to `invoices`. See the full SQL in Step P3 above.

**Step 0.6 — Verify build**

Run `cargo check -p shellwego-billing` — must pass with 0 errors before proceeding.

---

### Phase 1: `PaymentProvider` Trait

**File:** `crates/shellwego-billing/src/providers/mod.rs`

```rust
//! Payment provider implementations
//!
//! Each provider implements the `PaymentProvider` trait for charge
//! creation, refund processing, webhook verification, and status polling.

pub mod stripe;
pub mod paystack;
pub mod mpesa;
pub mod gcash;
pub mod upi;
pub mod mercadopago;
pub mod crypto;
#[cfg(test)]
pub mod mock;

use crate::{BillingError, PaymentResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of a refund operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResult {
    pub refund_id: String,
    pub status: RefundStatus,
    pub amount_cents_refunded: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RefundStatus {
    Pending,
    Succeeded,
    Failed,
}

/// A parsed webhook event from any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedWebhookEvent {
    /// Provider-agnostic event type
    pub event_type: WebhookEventType,
    /// Raw provider event type string
    pub provider_event_type: String,
    /// Invoice ID (if extractable from the event)
    pub invoice_id: Option<String>,
    /// Transaction/payment ID from the provider
    pub provider_payment_id: Option<String>,
    /// Transaction ID in our system
    pub transaction_id: Option<String>,
    /// Amount in cents
    pub amount_cents: Option<i64>,
    /// Currency code
    pub currency: Option<String>,
    /// Failure reason (if applicable)
    pub failure_reason: Option<String>,
    /// Customer ID
    pub customer_id: Option<String>,
    /// Metadata key-value pairs
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Normalized webhook event types across providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WebhookEventType {
    PaymentSucceeded,
    PaymentFailed,
    PaymentPending,
    PaymentRefunded,
    PaymentPartiallyRefunded,
    SubscriptionCreated,
    SubscriptionUpdated,
    SubscriptionCanceled,
    SubscriptionRenewed,
    DisputeCreated,
    DisputeWon,
    DisputeLost,
    Unknown,
}

/// Common payment charge request.
#[derive(Debug, Clone)]
pub struct ChargeRequest {
    pub invoice_id: String,
    pub customer_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub payment_token: String,
    pub description: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub idempotency_key: Option<String>,
}

/// Common refund request.
#[derive(Debug, Clone)]
pub struct RefundRequest {
    pub original_transaction_id: String,
    pub amount_cents: Option<i64>, // None = full refund
    pub reason: String,
    pub idempotency_key: Option<String>,
}

/// Trait that all payment providers must implement.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// Provider identifier (e.g., "stripe", "paystack", "mpesa")
    fn provider_name(&self) -> &str;

    /// Create a payment charge.
    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError>;

    /// Process a refund.
    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError>;

    /// Verify a webhook signature.
    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError>;

    /// Parse a provider-specific webhook payload into a normalized event.
    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError>;

    /// Poll the status of a payment (for async providers like M-Pesa, crypto).
    async fn check_payment_status(&self, provider_payment_id: &str)
        -> Result<PaymentStatus, BillingError>;
}

/// Payment status from provider polling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
    Refunded,
    PartiallyRefunded,
    Disputed,
    Unknown(String),
}
```

Add `async-trait = "0.1"` to `Cargo.toml` dependencies.

**Register module in `lib.rs`:**
```rust
pub mod providers;
pub use providers::{PaymentProvider, ChargeRequest, RefundRequest, ParsedWebhookEvent, WebhookEventType, PaymentStatus, RefundResult, RefundStatus};
```

---

### Phase 2: Stripe Provider

**File:** `crates/shellwego-billing/src/providers/stripe.rs`

```rust
//! Stripe payment provider
//!
//! Implements PaymentProvider for Stripe using direct HTTP calls to the
//! Stripe REST API (no SDK dependency for reduced binary size).

use super::*;
use crate::BillingError;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct StripeProvider {
    api_key: String,
    webhook_secret: Option<String>,
    http_client: reqwest::Client,
}

impl StripeProvider {
    pub fn new(api_key: String, webhook_secret: Option<String>) -> Self {
        Self {
            api_key,
            webhook_secret,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Stripe HTTP client creation"),
        }
    }
}

#[async_trait]
impl PaymentProvider for StripeProvider {
    fn provider_name(&self) -> &str { "stripe" }

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        let url = "https://api.stripe.com/v1/payment_intents";
        let mut params = vec![
            ("amount", request.amount_cents.to_string()),
            ("currency", request.currency.clone()),
            ("payment_method", request.payment_token.clone()),
            ("confirm", "true".to_string()),
            ("metadata[invoice_id]", request.invoice_id.clone()),
            ("metadata[customer_id]", request.customer_id.clone()),
            ("description", request.description),
        ];
        if let Some(key) = request.idempotency_key {
            params.push(("idempotency_key", key));
        }

        let resp = self.http_client
            .post(url)
            .basic_auth(&self.api_key, None::<&str>)
            .form(&params)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!("Stripe request failed: {}", e)))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await
            .map_err(|e| BillingError::ProviderError(format!("Stripe parse failed: {}", e)))?;

        if !status.is_success() {
            let msg = body.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Stripe error");
            return Ok(PaymentResult {
                success: false,
                transaction_id: body.get("id").and_then(|v| v.as_str()).map(String::from),
                message: msg.to_string(),
            });
        }

        let txn_id = body.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let stripe_status = body.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");

        Ok(PaymentResult {
            success: stripe_status == "succeeded" || stripe_status == "processing",
            transaction_id: Some(txn_id),
            message: format!("Stripe status: {}", stripe_status),
        })
    }

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        let url = "https://api.stripe.com/v1/refunds";
        let mut params = vec![
            ("payment_intent", request.original_transaction_id.clone()),
            ("reason", request.reason.clone()),
        ];
        if let Some(amount) = request.amount_cents {
            params.push(("amount", amount.to_string()));
        }

        let resp = self.http_client
            .post(url)
            .basic_auth(&self.api_key, None::<&str>)
            .form(&params)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!("Stripe refund failed: {}", e)))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| BillingError::ProviderError(format!("Stripe refund parse: {}", e)))?;

        Ok(RefundResult {
            refund_id: body.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            status: match body.get("status").and_then(|v| v.as_str()) {
                Some("succeeded") => RefundStatus::Succeeded,
                Some("pending") => RefundStatus::Pending,
                Some("failed") | _ => RefundStatus::Failed,
            },
            amount_cents_refunded: body.get("amount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            message: format!("Stripe refund: {:?}", body.get("status")),
        })
    }

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        // Extract signing secret
        let secret = self.webhook_secret.as_ref()
            .ok_or_else(|| BillingError::WebhookVerificationError(
                "No Stripe webhook secret configured".to_string(),
            ))?;

        // Parse "t=...,v1=..." header
        let mut timestamp = None;
        let mut v1_sig = None;
        for part in signature.split(',') {
            let part = part.trim();
            if let Some(ts) = part.strip_prefix("t=") { timestamp = Some(ts.to_string()); }
            else if let Some(sig) = part.strip_prefix("v1=") { v1_sig = Some(sig.to_string()); }
        }

        let timestamp = timestamp.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing timestamp in Stripe signature".to_string(),
        ))?;
        let v1_sig = v1_sig.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing v1 signature".to_string(),
        ))?;

        // Optional: reject webhooks older than 5 minutes
        if let Ok(ts) = timestamp.parse::<i64>() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if (now - ts).abs() > 300 {
                return Err(BillingError::WebhookVerificationError(
                    "Stripe webhook timestamp too old".to_string(),
                ));
            }
        }

        // HMAC-SHA256(secret, "{timestamp}.{payload}")
        let mut signed_payload = timestamp.clone();
        signed_payload.push('.');
        signed_payload.push_str(&String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| BillingError::WebhookVerificationError(format!("HMAC error: {}", e)))?;
        mac.update(signed_payload.as_bytes());
        let result = mac.finalize();
        let computed = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            result.into_bytes(),
        );

        // Constant-time comparison
        Ok(constant_time_compare(v1_sig.as_bytes(), computed.as_bytes()))
    }

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)
            .map_err(|e| BillingError::ProviderError(format!("Stripe webhook parse: {}", e)))?;

        let event_type_str = body.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let data_obj = body.get("data").and_then(|d| d.get("object"));
        let metadata = data_obj.and_then(|o| o.get("metadata"))
            .and_then(|m| serde_json::from_value(m.clone()).ok())
            .unwrap_or_default();

        let invoice_id = metadata.get("invoice_id")
            .or_else(|| data_obj.and_then(|o| o.get("invoice")))
            .and_then(|v| v.as_str())
            .map(String::from);

        let provider_payment_id = data_obj
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let event_type = match event_type_str.as_str() {
            "payment_intent.succeeded" => WebhookEventType::PaymentSucceeded,
            "payment_intent.payment_failed" => WebhookEventType::PaymentFailed,
            "payment_intent.processing" => WebhookEventType::PaymentPending,
            "charge.refunded" => WebhookEventType::PaymentRefunded,
            "charge.refund.updated" => {
                let refund_status = body.pointer("/data/object/status")
                    .and_then(|v| v.as_str());
                match refund_status {
                    Some("succeeded") => WebhookEventType::PaymentRefunded,
                    Some("partially_refunded") => WebhookEventType::PaymentPartiallyRefunded,
                    _ => WebhookEventType::Unknown,
                }
            }
            "charge.dispute.created" => WebhookEventType::DisputeCreated,
            "charge.dispute.won" => WebhookEventType::DisputeWon,
            "charge.dispute.closed" => WebhookEventType::DisputeLost,
            "customer.subscription.created" => WebhookEventType::SubscriptionCreated,
            "customer.subscription.updated" => WebhookEventType::SubscriptionUpdated,
            "customer.subscription.deleted" => WebhookEventType::SubscriptionCanceled,
            "invoice.payment_succeeded" => WebhookEventType::SubscriptionRenewed,
            _ => WebhookEventType::Unknown,
        };

        let amount_cents = data_obj
            .and_then(|o| o.get("amount"))
            .and_then(|v| v.as_i64());

        let currency = data_obj
            .and_then(|o| o.get("currency"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let failure_reason = data_obj
            .and_then(|o| o.get("last_payment_error"))
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(String::from);

        let customer_id = metadata.get("customer_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: event_type_str,
            invoice_id,
            provider_payment_id,
            transaction_id: provider_payment_id.clone(),
            amount_cents,
            currency,
            failure_reason,
            customer_id,
            metadata,
        })
    }

    async fn check_payment_status(&self, provider_payment_id: &str)
        -> Result<PaymentStatus, BillingError>
    {
        let url = format!("https://api.stripe.com/v1/payment_intents/{}", provider_payment_id);
        let resp = self.http_client
            .get(&url)
            .basic_auth(&self.api_key, None::<&str>)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!("Stripe status check: {}", e)))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| BillingError::ProviderError(format!("Stripe parse: {}", e)))?;

        Ok(match body.get("status").and_then(|v| v.as_str()) {
            Some("succeeded") => PaymentStatus::Succeeded,
            Some("processing") => PaymentStatus::Pending,
            Some("requires_payment_method") | Some("requires_confirmation") |
            Some("requires_action") => PaymentStatus::Pending,
            Some("canceled") => PaymentStatus::Failed,
            other => PaymentStatus::Unknown(other.unwrap_or("unknown").to_string()),
        })
    }
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
```

---

### Phase 3: Paystack Provider

**File:** `crates/shellwego-billing/src/providers/paystack.rs`

Implements `PaymentProvider` for Paystack (African payments).

**Key API endpoints:**
- `POST /v1/charge/` — Initialize charge with email, amount, authorization_code
- `POST /v1/charge/submit_pin` — Submit PIN for card payments
- `POST /v1/charge/submit_otp` — Submit OTP
- `GET /v1/transaction/{id}/verify` — Verify transaction
- `POST /v1/refund` — Issue refund
- Webhook: HMAC-SHA512 of raw body using secret key

**Charge flow:**
1. `POST /v1/transaction/initialize` with `{ email, amount, metadata: { invoice_id, customer_id } }`
2. Return `PaymentResult { success: false, transaction_id: Some(ref), message: "Redirect to: ..." }` (for redirect-based flow)
3. OR accept `authorization_code` for recurring card charges

**Webhook parsing:**
Paystack events use format:
```json
{
  "event": "charge.success",
  "data": {
    "reference": "ref_...",
    "status": "success",
    "amount": 10000,
    "currency": "NGN",
    "metadata": { "invoice_id": "..." }
  }
}
```

Map `event` field to `WebhookEventType`:
- `charge.success` → `PaymentSucceeded`
- `charge.failed` → `PaymentFailed`
- `transfer.success` → (ignored or custom)
- `transfer.failed` → (ignored)

---

### Phase 4: M-Pesa Provider

**File:** `crates/shellwego-billing/src/providers/mpesa.rs`

**API:** Safaricom Daraja API
- Base URL: `https://sandbox.safaricom.co.ke` (sandbox) or `https://api.safaricom.co.ke` (production)
- `POST /oauth/v1/generate?grant_type=client_credentials` — Get access token (Basic auth with `consumer_key:consumer_secret`)
- `POST /mpesa/stkpush/v1/processrequest` — Initiate STK push (Lipa Na M-Pesa)
- `GET /mpesa/stkpushquery/v1/query?CheckoutRequestID=...` — Check payment status

**Charge flow:**
1. Obtain OAuth token using `consumer_key:consumer_secret` Basic auth
2. `POST /mpesa/stkpush/v1/processrequest` with:
   ```json
   {
     "BusinessShortCode": "174379",
     "Password": "<base64(ShortCode+Passkey+Timestamp)>",
     "Timestamp": "20240101120000",
     "TransactionType": "CustomerPayBillOnline",
     "Amount": 100,
     "PartyA": "254712345678",
     "PartyB": "174379",
     "PhoneNumber": "254712345678",
     "CallBackURL": "https://api.shellwego.com/webhooks/mpesa",
     "AccountReference": "INV-202401-0001",
     "TransactionDesc": "Invoice payment"
   }
   ```
3. Response contains `CheckoutRequestID` — store as transaction ID
4. M-Pesa sends results to `CallBackURL` — parse webhook

**Status polling:**
`GET /mpesa/stkpushquery/v1/query?CheckoutRequestID=...`

**Webhook parsing:**
M-Pesa `C2B` or `STK` callback:
```json
{
  "Body": {
    "stkCallback": {
      "MerchantRequestID": "...",
      "CheckoutRequestID": "...",
      "ResultCode": 0,
      "ResultDesc": "...",
      "CallbackMetadata": {
        "Item": [
          { "Name": "Amount", "Value": 100 },
          { "Name": "MpesaReceiptNumber", "Value": "..." },
          { "Name": "PhoneNumber", "Value": "..." }
        ]
      }
    }
  }
}
```

`ResultCode == 0` → success. `ResultCode == 1032` → cancelled. `ResultCode == 1037` → timeout.

---

### Phase 5: GCash Provider

**File:** `crates/shellwego-billing/src/providers/gcash.rs`

**API:** PayMongo (primary GCash gateway)
- Base URL: `https://api.paymongo.com/v1`
- `POST /v1/sources` — Create payment source with type `gcash`
- `POST /v1/payments` — Create payment from source
- `GET /v1/payments/{id}` — Check status
- `POST /v1/refunds` — Issue refund
- Webhook: HMAC verification with webhook secret

**Charge flow:**
1. Create source:
   ```json
   POST /v1/sources
   {
     "data": {
       "attributes": {
         "type": "gcash",
         "amount": 10000,
         "currency": "PHP",
         "redirect": {
           "success": "https://...",
           "failed": "https://..."
         },
         "metadata": { "invoice_id": "..." }
       }
     }
   }
   ```
2. Response includes `redirect.checkout_url` — redirect user to GCash app
3. Create payment using source ID: `POST /v1/payments` with `{ "data": { "attributes": { "source": { "id": "src_..." }, "amount": 10000, "currency": "PHP" } } }`
4. GCash payment is async — webhook notifies completion

**Webhook parsing:**
```json
{
  "data": {
    "id": "pay_...",
    "type": "payment",
    "attributes": {
      "status": "paid",
      "amount": 10000,
      "currency": "PHP",
      "metadata": { "invoice_id": "..." }
    }
  },
  "livemode": false
}
```

---

### Phase 6: UPI Provider

**File:** `crates/shellwego-billing/src/providers/upi.rs`

**API:** Razorpay (primary UPI gateway)
- Base URL: `https://api.razorpay.com/v1`
- Auth: Basic auth with `key_id:key_secret`
- `POST /v1/orders` — Create order
- `POST /v1/payments/{id}/capture` — Capture authorized payment
- `GET /v1/payments/{id}` — Check status
- `POST /v1/refunds` — Issue refund
- Webhook: HMAC-SHA256 verification

**Charge flow:**
1. Create order:
   ```json
   POST /v1/orders
   {
     "amount": 10000,
     "currency": "INR",
     "receipt": "INV-202401-0001",
     "notes": { "invoice_id": "...", "customer_id": "..." }
   }
   ```
2. Return `order_id` to frontend — frontend uses Razorpay SDK to open UPI intent
3. After UPI payment, `payment.razorpay_payment_id` is available on frontend
4. Capture: `POST /v1/payments/{payment_id}/capture` with `{ "amount": 10000 }`
5. Webhook confirms payment

**Webhook parsing:**
```json
{
  "event": "payment.captured",
  "payload": {
    "payment": {
      "entity": {
        "id": "pay_...",
        "order_id": "order_...",
        "amount": 10000,
        "currency": "INR",
        "status": "captured",
        "notes": { "invoice_id": "..." }
      }
    }
  }
}
```

---

### Phase 7: Mercado Pago Provider

**File:** `crates/shellwego-billing/src/providers/mercadopago.rs`

**API:** Mercado Pago
- Base URL: `https://api.mercadopago.com/v1`
- Auth: Bearer token (`access_token`)
- `POST /v1/payments` — Create payment with payment_method_id
- `GET /v1/payments/{id}` — Check status
- `POST /v1/payments/{id}/refunds` — Issue refund
- Webhook: x-signature verification (SHA256 with `secret`)

**Charge flow:**
1. Create payment:
   ```json
   POST /v1/payments
   {
     "transaction_amount": 100.00,
     "payment_method_id": "pix",
     "payer": { "email": "user@example.com" },
     "description": "Invoice INV-202401-0001",
     "external_reference": "inv_123",
     "metadata": { "invoice_id": "inv_123" }
   }
   ```
2. Response includes `point_of_interaction.transaction_data.qr_code_base64` (for PIX)
3. Payment is async — webhook notifies completion

**Webhook parsing:**
```json
{
  "action": "payment.updated",
  "data": {
    "id": "123456789"
  }
}
```
Requires follow-up `GET /v1/payments/{id}` to get full payment details.

---

### Phase 8: Crypto Provider

**File:** `crates/shellwego-billing/src/providers/crypto.rs`

**API:** mempool.space (open, no API key) + optional CoinGecko for rates
- `GET https://mempool.space/api/address/{address}/txs` — Get transactions for address
- `GET https://mempool.space/api/tx/{txid}` — Get transaction details
- `GET https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,ethereum&vs_currencies=usd` — Get exchange rates

**Charge flow:**
1. On `process_crypto_payment`, generate a unique payment address (in production, derive from HD wallet; for MVP, use a shared wallet address with per-payment amounts to distinguish):
   - Calculate crypto amount from fiat using exchange rate API
   - Store expected amount + address + invoice_id mapping
   - Return `PaymentResult { success: false, transaction_id: Some(address), message: "Send X BTC to {address}" }`
2. Status polling (`check_payment_status`):
   - Query mempool.space for transactions to the payment address
   - Match by amount (within tolerance) or by OP_RETURN metadata
   - Count confirmations against `required_confirmations`
   - Return `PaymentStatus::Succeeded` when confirmed

**Amount conversion:**
```
crypto_amount = fiat_amount_cents / 100.0 / exchange_rate
```
Apply a 1% buffer to handle rate fluctuation:
```
expected_amount = crypto_amount * 0.99
```

**Confirmation monitoring:**
- Bitcoin: 3 confirmations (~30 minutes)
- Ethereum: 12 confirmations (~3 minutes)
- USDC (Polygon): 1 confirmation (~2 seconds)

**Webhook:**
Crypto doesn't have traditional webhooks. Implement a polling background task:
```rust
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Poll pending crypto payments
    }
});
```

---

### Phase 9: Mock Provider (Testing)

**File:** `crates/shellwego-billing/src/providers/mock.rs`

```rust
//! Mock payment provider for testing
//!
//! Returns configurable success/failure. Records all calls for assertions.

use super::*;
use std::sync::RwLock;

#[derive(Debug, Clone)]
pub struct MockProvider {
    pub charge_should_succeed: bool,
    pub charge_delay_ms: u64,
    pub transaction_id_prefix: String,
    pub calls: Arc<RwLock<MockCalls>>,
}

#[derive(Debug, Default)]
pub struct MockCalls {
    pub charges: Vec<ChargeRequest>,
    pub refunds: Vec<RefundRequest>,
    pub webhook_verifications: usize,
    pub status_checks: Vec<String>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            charge_should_succeed: true,
            charge_delay_ms: 0,
            transaction_id_prefix: "mock_txn_".to_string(),
            calls: Arc::new(RwLock::new(MockCalls::default())),
        }
    }

    pub fn failing() -> Self {
        let mut mock = Self::new();
        mock.charge_should_succeed = false;
        mock
    }
}

#[async_trait]
impl PaymentProvider for MockProvider {
    fn provider_name(&self) -> &str { "mock" }

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        if self.charge_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.charge_delay_ms)).await;
        }
        self.calls.write().unwrap().charges.push(request);
        Ok(PaymentResult {
            success: self.charge_should_succeed,
            transaction_id: Some(format!("{}{}", self.transaction_id_prefix, Uuid::new_v4())),
            message: if self.charge_should_succeed {
                "Mock: payment succeeded".to_string()
            } else {
                "Mock: card declined".to_string()
            },
        })
    }

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        self.calls.write().unwrap().refunds.push(request);
        Ok(RefundResult {
            refund_id: format!("mock_ref_{}", Uuid::new_v4()),
            status: RefundStatus::Succeeded,
            amount_cents_refunded: 0,
            message: "Mock: refund succeeded".to_string(),
        })
    }

    fn verify_webhook(&self, _payload: &[u8], _signature: &str) -> Result<bool, BillingError> {
        self.calls.write().unwrap().webhook_verifications += 1;
        Ok(true)
    }

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;
        Ok(ParsedWebhookEvent {
            event_type: WebhookEventType::PaymentSucceeded,
            provider_event_type: body.get("event_type")
                .and_then(|v| v.as_str())
                .unwrap_or("mock.event")
                .to_string(),
            invoice_id: body.get("invoice_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            provider_payment_id: body.get("transaction_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            transaction_id: None,
            amount_cents: body.get("amount_cents").and_then(|v| v.as_i64()),
            currency: body.get("currency").and_then(|v| v.as_str()).map(String::from),
            failure_reason: None,
            customer_id: None,
            metadata: Default::default(),
        })
    }

    async fn check_payment_status(&self, provider_payment_id: &str)
        -> Result<PaymentStatus, BillingError>
    {
        self.calls.write().unwrap().status_checks.push(provider_payment_id.to_string());
        Ok(PaymentStatus::Succeeded)
    }
}
```

---

### Phase 10: Refactor `BillingSystem` to Use Providers

**File:** `crates/shellwego-billing/src/lib.rs`

**Step 10.1 — Add provider registry to `BillingSystem`:**

```rust
use crate::providers::PaymentProvider;

pub struct BillingSystem {
    // ... existing fields ...
    /// Registered payment providers (keyed by provider name)
    providers: Arc<RwLock<HashMap<String, Arc<dyn PaymentProvider>>>>,
}
```

**Step 10.2 — Provider registration in `new()` and `new_with_pool()`:**

```rust
// After constructing self, register providers
let mut providers = HashMap::new();

// Always register mock provider for testing
// In production, register based on config
if let Some(ref key) = config.stripe_api_key {
    let stripe = providers::stripe::StripeProvider::new(
        key.clone(),
        config.stripe_webhook_secret.clone(),
    );
    providers.insert("stripe".to_string(), Arc::new(stripe) as Arc<dyn PaymentProvider>);
}
if let Some(ref secret) = config.paystack_secret_key {
    let paystack = providers::paystack::PaystackProvider::new(secret.clone());
    providers.insert("paystack".to_string(), Arc::new(paystack) as Arc<dyn PaymentProvider>);
}
if let Some(ref mpesa) = config.mpesa_config {
    let mp = providers::mpesa::MpesaProvider::new(mpesa.clone());
    providers.insert("mpesa".to_string(), Arc::new(mp) as Arc<dyn PaymentProvider>);
}
// ... similar for gcash, upi, mercadopago, crypto ...
```

**Step 10.3 — Replace `process_card_payment` dispatch:**

```rust
pub async fn process_payment(
    &self,
    invoice_id: &str,
    method: PaymentMethod,
    provider_name: Option<&str>,
) -> Result<PaymentResult, BillingError> {
    let invoice = self.get_invoice(invoice_id).await?;
    if invoice.status == InvoiceStatus::Paid {
        return Ok(PaymentResult { success: true, transaction_id: None, message: "Already paid".into() });
    }

    let provider = self.resolve_provider(method.clone(), provider_name).await?;
    let request = ChargeRequest {
        invoice_id: invoice_id.to_string(),
        customer_id: invoice.customer_id.clone(),
        amount_cents: (invoice.total * Decimal::ONE_HUNDRED).to_string().parse().unwrap_or(0),
        currency: invoice.currency.clone(),
        payment_token: self.extract_token(&method),
        description: format!("Invoice {}", invoice.invoice_number),
        metadata: HashMap::from([("invoice_id".to_string(), invoice_id.to_string())]),
        idempotency_key: Some(format!("inv_{}_{}", invoice_id, Uuid::new_v4())),
    };

    let result = provider.charge(request).await?;

    self.record_payment_attempt(&invoice, &method, &result).await?;
    if result.success {
        self.mark_invoice_paid(invoice_id, result.transaction_id.clone()).await?;
    }
    Ok(result)
}
```

**Step 10.4 — Replace `handle_webhook` dispatch:**

```rust
pub async fn handle_webhook(
    &self,
    provider_name: &str,
    payload: &[u8],
    signature: &str,
) -> Result<WebhookResult, BillingError> {
    let providers = self.providers.read().await;
    let provider = providers.get(provider_name)
        .ok_or_else(|| BillingError::WebhookVerificationError(
            format!("Unknown provider: {}", provider_name)
        ))?
        .clone();
    drop(providers); // Release read lock

    if !provider.verify_webhook(payload, signature)? {
        return Err(BillingError::WebhookVerificationError("Invalid signature".into()));
    }

    let event = provider.parse_webhook_event(payload)?;
    self.process_webhook_event(&event).await
}

async fn process_webhook_event(&self, event: &ParsedWebhookEvent) -> Result<WebhookResult, BillingError> {
    match event.event_type {
        WebhookEventType::PaymentSucceeded => {
            if let Some(ref invoice_id) = event.invoice_id {
                self.mark_invoice_paid(invoice_id, event.transaction_id.clone()).await?;
            }
        }
        WebhookEventType::PaymentFailed => {
            if let Some(ref invoice_id) = event.invoice_id {
                self.handle_payment_failure(
                    invoice_id,
                    event.failure_reason.as_deref().unwrap_or("Unknown"),
                ).await?;
            }
        }
        WebhookEventType::PaymentRefunded => {
            // Mark invoice as refunded (new status needed)
        }
        WebhookEventType::PaymentPending => {
            // Payment is processing, no action needed
        }
        _ => {
            info!(event_type = ?event.event_type, "Unhandled webhook event");
        }
    }
    Ok(WebhookResult { event_type: event.provider_event_type.clone(), processed: true })
}
```

**Step 10.5 — Add refund method:**

```rust
pub async fn refund_payment(
    &self,
    transaction_id: &str,
    provider_name: &str,
    amount_cents: Option<i64>,
    reason: &str,
) -> Result<RefundResult, BillingError> {
    let providers = self.providers.read().await;
    let provider = providers.get(provider_name)
        .ok_or_else(|| BillingError::ProviderError(format!("Unknown provider: {}", provider_name)))?
        .clone();
    drop(providers);

    provider.refund(RefundRequest {
        original_transaction_id: transaction_id.to_string(),
        amount_cents,
        reason: reason.to_string(),
        idempotency_key: Some(format!("refund_{}_{}", transaction_id, Uuid::new_v4())),
    }).await
}
```

---

### Phase 11: Invoice PDF Generation

**File:** `crates/shellwego-billing/src/invoices.rs`

Replace `headless_chrome` with `genpdf`:

```rust
#[cfg(feature = "pdf")]
pub async fn generate_pdf(&self, invoice: &Invoice) -> Result<Vec<u8>, BillingError> {
    use genpdf::{Document, elements, style, fonts};

    // Load a bundled font (or use the system default)
    let font_data = fonts::FontData::new(
        fonts::builtin::BUILTINS.by_regular.take().unwrap(),
        None, // No bold variant needed for this use case
    );
    let font_family = fonts::FontFamily::from_font_data(font_data, None, None, None);

    let mut doc = Document::new(font_family);
    doc.set_title(format!("Invoice {}", invoice.invoice_number));

    // Header
    doc.push(elements::Paragraph::new(&self.branding.company_name)
        .styled(style::Style::new().with_font_size(24.0)));
    doc.push(elements::Break::new(1.0));

    // Invoice number and date
    doc.push(elements::Paragraph::new(&format!(
        "Invoice: {}  |  Date: {}  |  Due: {}",
        invoice.invoice_number,
        invoice.created_at.format("%B %d, %Y"),
        invoice.due_date.format("%B %d, %Y"),
    )).styled(style::Style::new().with_font_size(11.0)));
    doc.push(elements::Break::new(0.5));

    // Customer info
    doc.push(elements::Paragraph::new(&format!(
        "Bill To: {} ({})",
        invoice.customer_name,
        invoice.customer_email,
    )).styled(style::Style::new().with_font_size(11.0)));
    doc.push(elements::Break::new(0.5));

    // Line items table
    let mut table = elements::Table::new(vec![
        (style::Style::new().bold(), 2.0),
        (style::Style::new().bold(), 1.0),
        (style::Style::new().bold(), 1.0),
        (style::Style::new().bold(), 1.5),
        (style::Style::new().bold(), 1.5),
    ]);

    // Header row
    table.push_row(elements::TableRow::new(vec![
        elements::TableCell::new("Description").with_style(style::Style::new().bold()),
        elements::TableCell::new("Qty").with_style(style::Style::new().bold()),
        elements::TableCell::new("Unit").with_style(style::Style::new().bold()),
        elements::TableCell::new("Unit Price").with_style(style::Style::new().bold()),
        elements::TableCell::new("Amount").with_style(style::Style::new().bold()),
    ]));

    // Data rows
    for item in &invoice.line_items {
        table.push_row(elements::TableRow::new(vec![
            elements::TableCell::new(&item.description),
            elements::TableCell::new(&format!("{:.2}", item.quantity)),
            elements::TableCell::new(&item.unit),
            elements::TableCell::new(&format!("${:.4}", item.unit_price)),
            elements::TableCell::new(&item.amount.to_string()),
        ]));
    }
    doc.push(table);
    doc.push(elements::Break::new(0.5));

    // Totals
    doc.push(elements::Paragraph::new(&format!(
        "Subtotal: {} {}", invoice.subtotal, invoice.currency
    )));
    if invoice.credit_applied > Decimal::ZERO {
        doc.push(elements::Paragraph::new(&format!(
            "Credits: -{} {}", invoice.credit_applied, invoice.currency
        )));
    }
    doc.push(elements::Paragraph::new(&format!(
        "Total: {} {}",
        invoice.total, invoice.currency
    )).styled(style::Style::new().bold().with_font_size(14.0)));

    // Footer
    if let Some(ref footer) = self.branding.footer {
        doc.push(elements::Break::new(1.0));
        doc.push(elements::Paragraph::new(footer)
            .styled(style::Style::new().with_font_size(9.0)));
    }

    // Render to bytes
    let mut bytes = Vec::new();
    doc.render(&mut bytes)
        .map_err(|e| BillingError::InvoiceError(format!("PDF render failed: {}", e)))?;

    Ok(bytes)
}
```

---

### Phase 12: Background Payment Polling

**File:** `crates/shellwego-billing/src/lib.rs`, extend `run_workers`

Add a fourth background worker for polling async payment providers (M-Pesa, GCash, crypto):

```rust
// Async payment polling worker (runs every 60 seconds)
let providers = self.providers.clone();
let pool = self.pool.clone();
tokio::spawn(async move {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        if let Some(ref pool) = pool {
            // Query pending payments
            let rows = match sqlx::query(
                "SELECT id, provider, transaction_id FROM payments WHERE status = 'pending'"
            ).fetch_all(pool).await {
                Ok(r) => r,
                Err(e) => { error!(error = %e, "Failed to query pending payments"); continue; }
            };

            for row in rows {
                let payment_id: String = row.get("id");
                let provider_name: String = row.get("provider");
                let provider_tx_id: String = row.get("transaction_id");

                let prov_read = providers.read().await;
                if let Some(provider) = prov_read.get(&provider_name) {
                    match provider.check_payment_status(&provider_tx_id).await {
                        Ok(PaymentStatus::Succeeded) => {
                            // Update payment and invoice status
                            let _ = sqlx::query(
                                "UPDATE payments SET status = 'succeeded', updated_at = NOW() WHERE id = $1"
                            ).bind(&payment_id).execute(pool).await;
                        }
                        Ok(PaymentStatus::Failed) => {
                            let _ = sqlx::query(
                                "UPDATE payments SET status = 'failed', updated_at = NOW() WHERE id = $1"
                            ).bind(&payment_id).execute(pool).await;
                        }
                        _ => {} // Still pending, skip
                    }
                }
            }
        }
    }
});
```

---

## 6. Dependencies on Other Plans

| Plan ID | Dependency | Notes |
|---|---|---|
| **01** (Security Hardening) | Medium — RBAC for billing endpoints | Admin-only operations (`refund_payment`, `upsert_pricing_plan`) should require admin role. If Plan 01 wires RBAC, this plan's CLI/API layer should check permissions. If not, a TODO comment marks the integration point. |
| **03** (QUIC Message Bus) | Low — event notifications | Payment success/failure events could be published to the message bus for other services to consume (e.g., provisioning on payment success). This plan focuses on the billing crate itself; message bus integration is a follow-up. |
| **05** (Schema Consolidation) | Low — type alignment | Adding `transaction_id` to `Invoice` and new config structs aligns with schema consolidation. Ensure new types are in `shellwego-schema` and re-exported. |
| **06** (CLI Completion) | Low — CLI integration | `shellwego pricing set` and `shellwego billing refund` CLI commands would call the billing API. This plan provides the backend; Plan 06 provides the CLI frontend. |

**Recommended execution order:** This plan is independent of Plans 01-06 but should be executed after any schema consolidation (Plan 05) to avoid merge conflicts on schema types. The build fix (Phase 0) can land independently and immediately.

## 7. Acceptance Criteria

### Build & Compilation
- [ ] `cargo check -p shellwego-billing` passes with 0 errors
- [ ] `cargo check -p shellwego-billing --features pdf` passes with 0 errors
- [ ] `cargo check -p shellwego-billing --features stripe` passes with 0 errors
- [ ] `cargo check -p shellwego-billing --all-features` passes with 0 errors
- [ ] `cargo test -p shellwego-billing` passes with 0 test failures
- [ ] `cargo test -p shellwego-schema` passes with 0 test failures (after Invoice change)

### Provider Architecture
- [ ] `PaymentProvider` trait compiles and is exported from `shellwego-billing`
- [ ] `StripeProvider` implements all trait methods with real HTTP calls
- [ ] `PaystackProvider` implements all trait methods with real HTTP calls
- [ ] `MpesaProvider` implements all trait methods with Daraja API integration
- [ ] `GcashProvider` implements all trait methods via PayMongo
- [ ] `UpiProvider` implements all trait methods via Razorpay
- [ ] `MercadoPagoProvider` implements all trait methods
- [ ] `CryptoProvider` implements charge (address + expected amount), status polling, and exchange rate conversion
- [ ] `MockProvider` records all calls and returns configurable results
- [ ] `BillingSystem::process_payment` dispatches to the correct provider based on `PaymentMethod` and `provider_name`
- [ ] `BillingSystem::handle_webhook` dispatches to the correct provider's `verify_webhook` and `parse_webhook_event`
- [ ] `BillingSystem::refund_payment` calls the provider's `refund` method

### Webhook Processing
- [ ] Stripe webhooks: `payment_intent.succeeded`, `payment_intent.payment_failed`, `charge.refunded` are correctly parsed
- [ ] Paystack webhooks: `charge.success`, `charge.failed` are correctly parsed
- [ ] M-Pesa STK callbacks: `ResultCode == 0` maps to `PaymentSucceeded`
- [ ] Unknown providers return clear error: "Unknown provider: X"
- [ ] Webhook verification uses `stripe_webhook_secret` when available, falls back to `stripe_api_key`
- [ ] Timestamp tolerance (5 minutes) is enforced for Stripe webhooks

### Invoice PDF
- [ ] `generate_pdf` with `pdf` feature produces valid PDF bytes (not HTML)
- [ ] PDF contains invoice number, date, customer info, line items, totals
- [ ] PDF without `pdf` feature returns HTML bytes (unchanged fallback)
- [ ] `render_html` continues to produce well-formed HTML with the embedded Tera template

### Schema & Database
- [ ] `Invoice` struct includes `transaction_id: Option<String>`
- [ ] `row_to_invoice` reads `transaction_id` from database and assigns to struct
- [ ] `mark_invoice_paid` updates both `transaction_id` and `status` in database and in-memory
- [ ] Migration SQL `003_add_billing_tables.sql` uses PostgreSQL types (not SQLite)
- [ ] `BillingConfig` includes `stripe_webhook_secret`, `mpesa_config`, `gcash_config`, `upi_config`, `mercadopago_config`, `crypto_config`

### Refund Support
- [ ] `refund_payment` creates a refund record in the `payments` table with status `refunded`
- [ ] Partial refund is supported via `amount_cents: Some(n)`
- [ ] Full refund is triggered by `amount_cents: None`
- [ ] Refund is recorded with `original_transaction_id` reference

### Tests
- [ ] Unit test: `MockProvider` charge succeeds and records call
- [ ] Unit test: `MockProvider` charge fails and records call
- [ ] Unit test: `MockProvider` webhook verification always returns true
- [ ] Unit test: `StripeProvider::verify_webhook` with correct signature → true
- [ ] Unit test: `StripeProvider::verify_webhook` with tampered payload → false
- [ ] Unit test: `StripeProvider::verify_webhook` with expired timestamp → error
- [ ] Unit test: `StripeProvider::parse_webhook_event` for `payment_intent.succeeded`
- [ ] Unit test: `StripeProvider::parse_webhook_event` for `payment_intent.payment_failed`
- [ ] Unit test: `PaystackProvider::verify_webhook` with correct signature → true
- [ ] Unit test: `PaystackProvider::parse_webhook_event` for `charge.success`
- [ ] Unit test: `MpesaProvider` password generation (base64 encoding)
- [ ] Unit test: Invoice PDF generation (verify non-empty bytes)
- [ ] Unit test: Crypto exchange rate conversion (fiat → crypto amount)
- [ ] Unit test: `constant_time_compare` returns false for different lengths
- [ ] Integration test: `BillingSystem` with mock provider processes payment end-to-end
- [ ] Integration test: `BillingSystem` with mock provider handles webhook end-to-end

## 8. Estimated Complexity

**XL** (Extra-Large)

Rationale:
- **Phase 0** (build fix + schema alignment): ~150 lines changed across 5 files. Mechanical but touches the core struct. Medium complexity.
- **Phase 1** (PaymentProvider trait): ~180 lines new. Medium complexity (trait design, normalization types).
- **Phase 2** (Stripe provider): ~300 lines new. Medium complexity (HTTP calls, webhook parsing, refund).
- **Phase 3** (Paystack provider): ~250 lines new. Medium complexity (similar pattern to Stripe).
- **Phase 4** (M-Pesa provider): ~300 lines new. High complexity (OAuth flow, STK push, password encoding, callback parsing).
- **Phase 5** (GCash provider): ~200 lines new. Medium complexity (PayMongo API, redirect flow).
- **Phase 6** (UPI provider): ~250 lines new. Medium complexity (Razorpay order/capture flow).
- **Phase 7** (Mercado Pago provider): ~250 lines new. Medium complexity (payment creation, follow-up for webhook).
- **Phase 8** (Crypto provider): ~350 lines new. High complexity (exchange rate API, address handling, confirmation polling, amount tolerance).
- **Phase 9** (Mock provider): ~120 lines new. Low complexity.
- **Phase 10** (BillingSystem refactor): ~300 lines changed. High complexity (provider registry, dispatch, refactor existing methods without breaking tests).
- **Phase 11** (PDF generation): ~150 lines changed. Medium complexity (genpdf integration, layout).
- **Phase 12** (Background polling): ~80 lines new. Medium complexity (async task, DB queries).

**Total: ~2,880 lines of production code** across 14 files (9 new, 5 modified).

The main risk is the `BillingSystem` refactor in Phase 10 — the existing `process_card_payment`, `process_bank_transfer`, `process_wallet_payment`, `process_crypto_payment`, `verify_stripe_webhook`, and `verify_paystack_webhook` methods must be preserved behind the trait dispatch. Tests that directly call these private methods (like `verify_stripe_webhook` tests at lines 2060-2105) must be updated to go through the public `handle_webhook` API or the provider's `verify_webhook` method directly.

## 9. Risk & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Provider API changes (Stripe v2, Paystack v2, etc.) | Medium | High | All providers are behind the `PaymentProvider` trait. Changes to one provider's API require updating only that file. The trait interface stays stable. |
| `genpdf` does not produce print-quality PDFs | Low | Medium | The embedded HTML template + headless Chrome approach is retained as a `headless-chrome` feature flag fallback. Users who need pixel-perfect invoices can enable `features = ["headless-chrome"]` instead of `features = ["pdf"]`. |
| Crypto exchange rate volatility during payment window | Medium | Medium | Apply a 1% buffer on expected crypto amount. Set `confirmation_timeout_minutes` per currency (e.g., 60 min for BTC). If rate drifts beyond 2%, require re-initiation. |
| M-Pesa sandbox API changes | Medium | Low | M-Pesa Daraja API is stable. Pin the API version in the base URL if needed. |
| BillingSystem refactor breaks existing callers | Low | High | Keep old method signatures as deprecated wrappers that delegate to the new provider dispatch. Run `cargo test` after each phase. |
| Payment provider secrets leaked in logs | Low | Critical | All provider constructors accept secrets via `String`. Ensure `tracing` instrumentation in provider methods does NOT log secrets. Use `#[instrument(skip(config, secret))]` attributes. |
| PDF generation with `genpdf` has font issues | Medium | Low | Bundle a minimal font (e.g., `Roboto`) as a static byte slice using `include_bytes!`. Fallback to the embedded HTML template. |
