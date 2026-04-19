-- Add transaction_kind to distinguish deposit/withdraw/transfer explicitly.
-- This avoids ambiguous interpretation when from_account_id == to_account_id.

ALTER TABLE transactions
    ADD COLUMN IF NOT EXISTS transaction_kind VARCHAR(20);

-- Backfill existing rows as best-effort:
-- - if from != to => transfer
-- - if from == to => deposit (historically our deposit/withdraw both used from==to; assume deposit)
UPDATE transactions
SET transaction_kind = CASE
    WHEN from_account_id <> to_account_id THEN 'transfer'
    ELSE 'deposit'
END
WHERE transaction_kind IS NULL;

ALTER TABLE transactions
    ALTER COLUMN transaction_kind SET NOT NULL;

ALTER TABLE transactions
    ADD CONSTRAINT transactions_transaction_kind_check
        CHECK (transaction_kind IN ('deposit', 'withdraw', 'transfer'));

CREATE INDEX IF NOT EXISTS idx_transactions_transaction_kind
    ON transactions(transaction_kind);

