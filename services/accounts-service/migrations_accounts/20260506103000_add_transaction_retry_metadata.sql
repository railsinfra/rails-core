ALTER TABLE transactions
    ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_attempted_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS next_retry_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS terminal_failure_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_transactions_status_next_retry_updated
    ON transactions (status, next_retry_at, updated_at);
