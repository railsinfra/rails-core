ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS organization_id UUID,
    ADD COLUMN IF NOT EXISTS environment VARCHAR(20) NOT NULL DEFAULT 'production' CHECK (environment IN ('sandbox', 'production'));

CREATE INDEX IF NOT EXISTS idx_accounts_org_env_user_id ON accounts(organization_id, environment, user_id);
