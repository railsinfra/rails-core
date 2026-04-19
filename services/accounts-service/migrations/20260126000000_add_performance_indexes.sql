-- Add indexes to optimize slow queries in accounts service

-- Transactions table indexes
-- Composite index for retry worker query: WHERE status = 'pending' AND created_at < $1 ORDER BY created_at ASC
-- Partial index for pending transactions only (more efficient)
CREATE INDEX IF NOT EXISTS transactions_status_created_at_pending_idx 
    ON transactions(status, created_at) 
    WHERE status = 'pending';

-- Index for status filtering (used in various queries)
CREATE INDEX IF NOT EXISTS transactions_status_idx 
    ON transactions(status) 
    WHERE status IN ('pending', 'posted', 'failed');

-- Index for created_at ordering (used in retry worker)
CREATE INDEX IF NOT EXISTS transactions_created_at_idx 
    ON transactions(created_at);

-- Composite index for organization + status queries (if needed)
-- Already exists: idx_transactions_org_status_created_at

-- Index for transaction id lookups (primary key should be fast, but ensure it's optimized)
-- Primary key already provides this, but we can verify it's being used

-- Accounts table indexes
-- Composite index for user accounts query: WHERE user_id = $1 ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS accounts_user_id_created_at_idx 
    ON accounts(user_id, created_at DESC);

-- Index for account_number lookups (already unique, but ensure index exists)
-- Already exists: idx_accounts_account_number (unique)

-- Index for id lookups (primary key should be fast, but adding explicit index for clarity)
-- Primary key already provides this, but we can add a covering index if needed

-- Index for status filtering
-- Already exists: idx_accounts_status

-- Composite index for organization + environment + user queries
-- Already exists: idx_accounts_org_env_user_id

-- Optimize account_number existence check (already has unique index, but ensure it's being used)
-- The unique constraint on account_number should already provide an index
