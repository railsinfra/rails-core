-- Allow a distinct "posting" state so only one actor (HTTP or retry worker) owns an in-flight ledger post.
-- Kept in sync with migrations_accounts/20260419120000_add_transaction_posting_status.sql
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_status_check;
ALTER TABLE transactions
    ADD CONSTRAINT transactions_status_check
    CHECK (status IN ('pending', 'posting', 'posted', 'failed'));
