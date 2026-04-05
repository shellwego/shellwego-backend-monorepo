-- 003_add_billing_tables.sql
-- Billing tables for Phase 5: real payment processing and database persistence.
-- Uses PostgreSQL syntax with TIMESTAMPTZ, JSONB, and proper constraints.

-- ============================================================
-- billing_customers: persistent customer records
-- ============================================================
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

-- ============================================================
-- invoices: invoice records with line items stored as JSONB
-- ============================================================
CREATE TABLE IF NOT EXISTS invoices (
    id                  VARCHAR(255) PRIMARY KEY,
    invoice_number      VARCHAR(64)  NOT NULL UNIQUE,
    customer_id         VARCHAR(255) NOT NULL,
    customer_name       VARCHAR(512) NOT NULL,
    customer_email      VARCHAR(512) NOT NULL,
    period_start        TIMESTAMPTZ  NOT NULL,
    period_end          TIMESTAMPTZ  NOT NULL,
    line_items_json     JSONB         NOT NULL DEFAULT '[]'::jsonb,
    subtotal            NUMERIC(20, 6) NOT NULL DEFAULT 0,
    credit_applied      NUMERIC(20, 6) NOT NULL DEFAULT 0,
    total               NUMERIC(20, 6) NOT NULL DEFAULT 0,
    currency            VARCHAR(3)   NOT NULL DEFAULT 'USD',
    status              VARCHAR(32)  NOT NULL DEFAULT 'Draft',
    due_date            TIMESTAMPTZ  NOT NULL,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    paid_at             TIMESTAMPTZ  DEFAULT NULL,
    transaction_id      VARCHAR(255) DEFAULT NULL
);

-- ============================================================
-- payments: payment attempt records with retry tracking
-- ============================================================
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

-- ============================================================
-- pricing_plans: database-backed pricing configuration
-- ============================================================
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

-- ============================================================
-- subscriptions: customer subscription records
-- ============================================================
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

-- ============================================================
-- Indexes
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
CREATE INDEX IF NOT EXISTS idx_payments_next_retry_at     ON payments(next_retry_at) WHERE status = 'failed' AND next_retry_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_pricing_plans_resource_type ON pricing_plans(resource_type) WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_subscriptions_customer_id   ON subscriptions(customer_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status        ON subscriptions(status);
