-- Remove balance persistence from accounts
ALTER TABLE accounts
    DROP COLUMN IF EXISTS balance;

-- Replace transactions table with intent-only schema
DROP TABLE IF EXISTS transactions;

CREATE TABLE IF NOT EXISTS transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,

    from_account_id UUID NOT NULL,
    to_account_id UUID NOT NULL,

    amount BIGINT NOT NULL,
    currency VARCHAR(3) NOT NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'posted', 'failed')),
    failure_reason TEXT,

    idempotency_key VARCHAR(255) NOT NULL,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_transactions_org_idempotency_key
    ON transactions(organization_id, idempotency_key);

CREATE INDEX IF NOT EXISTS idx_transactions_org_status_created_at
    ON transactions(organization_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_transactions_from_account_id
    ON transactions(from_account_id);

CREATE INDEX IF NOT EXISTS idx_transactions_to_account_id
    ON transactions(to_account_id);
