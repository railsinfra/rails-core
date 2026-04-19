-- Allow a distinct "posting" state so only one actor (HTTP or retry worker) owns an in-flight ledger post.
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_status_check;
ALTER TABLE transactions
    ADD CONSTRAINT transactions_status_check
    CHECK (status IN ('pending', 'posting', 'posted', 'failed'));
