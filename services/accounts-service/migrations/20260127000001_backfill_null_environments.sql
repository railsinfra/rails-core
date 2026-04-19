-- Backfill NULL environments to 'sandbox' (safety: never default to production)
-- This migration is idempotent and safe to run multiple times

UPDATE accounts
SET environment = 'sandbox'
WHERE environment IS NULL;

-- Ensure environment column is NOT NULL going forward
ALTER TABLE accounts
    ALTER COLUMN environment SET NOT NULL;

-- Add check constraint if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint 
        WHERE conname = 'accounts_environment_check'
    ) THEN
        ALTER TABLE accounts
            ADD CONSTRAINT accounts_environment_check 
            CHECK (environment IN ('sandbox', 'production'));
    END IF;
END $$;
