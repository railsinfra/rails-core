-- Add environment column to transactions table for sandbox/production separation.
-- This migration is backward-compatible: environment is nullable to support legacy transactions.

-- Add nullable environment column
ALTER TABLE transactions
    ADD COLUMN IF NOT EXISTS environment VARCHAR(20);

-- Add check constraint to ensure valid environment values (or NULL for legacy)
ALTER TABLE transactions
    DROP CONSTRAINT IF EXISTS transactions_environment_check;

ALTER TABLE transactions
    ADD CONSTRAINT transactions_environment_check
        CHECK (environment IN ('sandbox', 'production') OR environment IS NULL);

-- PostgreSQL's ON CONFLICT works best with unique constraints on actual columns.
-- We'll create a unique constraint that handles both NULL and non-NULL environments.
-- Since we can't create partial unique constraints directly, we'll use a workaround:
-- Create a unique index with COALESCE, then use application-level conflict handling,
-- OR use a simpler approach with a regular unique constraint.

-- Drop the old unique index first
DROP INDEX IF EXISTS idx_transactions_org_idempotency_key;

-- Create a unique index using COALESCE to handle NULL environments
-- This ensures uniqueness: NULL environment becomes '' in the index
CREATE UNIQUE INDEX IF NOT EXISTS idx_transactions_org_env_idempotency_key
    ON transactions(organization_id, COALESCE(environment, ''), idempotency_key);

-- Also create a partial unique index for legacy transactions for query performance
CREATE UNIQUE INDEX IF NOT EXISTS idx_transactions_org_idempotency_key
    ON transactions(organization_id, idempotency_key)
    WHERE environment IS NULL;

-- Add index for environment filtering
CREATE INDEX IF NOT EXISTS idx_transactions_environment
    ON transactions(environment)
    WHERE environment IS NOT NULL;

-- Add composite index for environment + status + created_at (useful for retry worker)
CREATE INDEX IF NOT EXISTS idx_transactions_env_status_created_at
    ON transactions(environment, status, created_at)
    WHERE environment IS NOT NULL;
