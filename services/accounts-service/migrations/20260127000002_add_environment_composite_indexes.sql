-- Add composite indexes with environment as leading column for efficient filtering
-- These indexes optimize queries that filter by environment + other criteria

-- Index for user_id queries with environment filter
-- Query pattern: WHERE user_id = $1 AND environment = $2 ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_accounts_env_user_created 
    ON accounts (environment, user_id, created_at DESC);

-- Index for organization_id queries with environment filter
-- Query pattern: WHERE organization_id = $1 AND environment = $2 ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_accounts_env_org_created 
    ON accounts (environment, organization_id, created_at DESC);

-- Index for admin_user_id queries with environment filter
-- Query pattern: WHERE admin_user_id = $1 AND environment = $2 ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_accounts_env_admin_created 
    ON accounts (environment, admin_user_id, created_at DESC);

-- Index for id lookups with environment filter (for get_account)
-- Query pattern: WHERE id = $1 AND environment = $2
CREATE INDEX IF NOT EXISTS idx_accounts_env_id 
    ON accounts (environment, id);

-- Note: The existing index idx_accounts_org_env_user_id may still be useful
-- but the new indexes above are optimized for environment-first filtering
