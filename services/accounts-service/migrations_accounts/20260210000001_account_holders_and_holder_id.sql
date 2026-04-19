-- Account holders: one row per logical holder per (organization_id, environment), keyed by UUID.
-- Email is unique per (organization_id, environment) but the primary key is id (UUID).
CREATE TABLE IF NOT EXISTS account_holders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    environment VARCHAR(20) NOT NULL CHECK (environment IN ('sandbox', 'production')),
    email VARCHAR(255) NOT NULL,
    first_name VARCHAR(255) NOT NULL DEFAULT '',
    last_name VARCHAR(255) NOT NULL DEFAULT '',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE (organization_id, environment, email)
);

CREATE INDEX IF NOT EXISTS idx_account_holders_org_env_email ON account_holders(organization_id, environment, email);

-- Add holder_id to accounts; make user_id nullable for holder-based accounts.
ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS holder_id UUID REFERENCES account_holders(id);

ALTER TABLE accounts
    ALTER COLUMN user_id DROP NOT NULL;

CREATE INDEX IF NOT EXISTS idx_accounts_holder_id ON accounts(holder_id) WHERE holder_id IS NOT NULL;
