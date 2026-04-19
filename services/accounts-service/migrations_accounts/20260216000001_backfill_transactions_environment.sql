-- Backfill NULL environments on transactions to 'sandbox' so we can enforce NOT NULL.
-- Eliminates COALESCE-based idempotency collision between legacy (NULL) and env-specific rows.

UPDATE transactions
SET environment = 'sandbox'
WHERE environment IS NULL;

-- Drop old COALESCE-based and partial indexes
DROP INDEX IF EXISTS idx_transactions_org_env_idempotency_key;
DROP INDEX IF EXISTS idx_transactions_org_idempotency_key;

-- Environment is now required; add NOT NULL
ALTER TABLE transactions
    ALTER COLUMN environment SET NOT NULL;

-- Update check constraint: no longer allow NULL
ALTER TABLE transactions
    DROP CONSTRAINT IF EXISTS transactions_environment_check;

ALTER TABLE transactions
    ADD CONSTRAINT transactions_environment_check
    CHECK (environment IN ('sandbox', 'production'));

-- New unique index: (org, environment, idempotency_key) - no COALESCE
CREATE UNIQUE INDEX idx_transactions_org_env_idempotency_key
    ON transactions(organization_id, environment, idempotency_key);
