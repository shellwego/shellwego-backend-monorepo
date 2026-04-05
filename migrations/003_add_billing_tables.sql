-- 003_add_billing_tables.sql
-- Billing tables for Phase 5: real payment processing and database persistence.
-- Compatible with BOTH SQLite and PostgreSQL (uses TEXT/INTEGER/REAL types).

-- ============================================================
-- billing_customers: persistent customer records
-- ============================================================
CREATE TABLE IF NOT EXISTS billing_customers (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    email           TEXT NOT NULL,
    address_json    TEXT DEFAULT NULL,
    tier            TEXT NOT NULL DEFAULT 'Free',
    credits         INTEGER NOT NULL DEFAULT 0,
    currency        TEXT NOT NULL DEFAULT 'USD',
    tax_id          TEXT DEFAULT NULL,
    status          TEXT NOT NULL DEFAULT 'Active',
    payment_methods_json TEXT DEFAULT '[]',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================
-- invoices: invoice records with line items stored as JSON
-- ============================================================
CREATE TABLE IF NOT EXISTS invoices (
    id                  TEXT PRIMARY KEY,
    invoice_number      TEXT NOT NULL UNIQUE,
    customer_id         TEXT NOT NULL,
    customer_name       TEXT NOT NULL,
    customer_email      TEXT NOT NULL,
    period_start        TEXT NOT NULL,
    period_end          TEXT NOT NULL,
    line_items_json     TEXT NOT NULL DEFAULT '[]',
    subtotal            REAL NOT NULL DEFAULT 0,
    credit_applied      REAL NOT NULL DEFAULT 0,
    total               REAL NOT NULL DEFAULT 0,
    currency            TEXT NOT NULL DEFAULT 'USD',
    status              TEXT NOT NULL DEFAULT 'Draft',
    due_date            TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    paid_at             TEXT DEFAULT NULL,
    transaction_id      TEXT DEFAULT NULL
);

-- ============================================================
-- payments: payment attempt records with retry tracking
-- ============================================================
CREATE TABLE IF NOT EXISTS payments (
    id                      TEXT PRIMARY KEY,
    invoice_id              TEXT DEFAULT NULL,
    customer_id             TEXT NOT NULL,
    amount_cents            INTEGER NOT NULL DEFAULT 0,
    currency                TEXT NOT NULL DEFAULT 'USD',
    method_type             TEXT NOT NULL DEFAULT 'card',
    provider                TEXT NOT NULL DEFAULT 'stripe',
    status                  TEXT NOT NULL DEFAULT 'pending',
    transaction_id          TEXT DEFAULT NULL,
    provider_response_json  TEXT DEFAULT NULL,
    retry_count             INTEGER NOT NULL DEFAULT 0,
    next_retry_at           TEXT DEFAULT NULL,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================
-- pricing_plans: database-backed pricing configuration
-- ============================================================
CREATE TABLE IF NOT EXISTS pricing_plans (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    price_cents     INTEGER NOT NULL DEFAULT 0,
    currency        TEXT NOT NULL DEFAULT 'USD',
    description     TEXT DEFAULT '',
    tier_multiplier REAL DEFAULT 1.0,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================
-- subscriptions: customer subscription records
-- ============================================================
CREATE TABLE IF NOT EXISTS subscriptions (
    id                      TEXT PRIMARY KEY,
    customer_id             TEXT NOT NULL,
    plan_id                 TEXT DEFAULT NULL,
    status                  TEXT NOT NULL DEFAULT 'active',
    current_period_start    TEXT NOT NULL DEFAULT (datetime('now')),
    current_period_end      TEXT NOT NULL DEFAULT (datetime('now')),
    provider_subscription_id TEXT DEFAULT NULL,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================
-- Indexes (plain indexes for SQLite compatibility)
-- ============================================================
CREATE INDEX IF NOT EXISTS idx_billing_customers_email    ON billing_customers(email);
CREATE INDEX IF NOT EXISTS idx_billing_customers_status   ON billing_customers(status);
CREATE INDEX IF NOT EXISTS idx_billing_customers_tier     ON billing_customers(tier);

CREATE INDEX IF NOT EXISTS idx_invoices_customer_id       ON invoices(customer_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status            ON invoices(status);
CREATE INDEX IF NOT EXISTS idx_invoices_invoice_number    ON invoices(invoice_number);
CREATE INDEX IF NOT EXISTS idx_invoices_due_date          ON invoices(due_date);

CREATE INDEX IF NOT EXISTS idx_payments_invoice_id        ON payments(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payments_customer_id       ON payments(customer_id);
CREATE INDEX IF NOT EXISTS idx_payments_status            ON payments(status);

CREATE INDEX IF NOT EXISTS idx_subscriptions_customer_id   ON subscriptions(customer_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status        ON subscriptions(status);
