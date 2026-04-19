-- Add external_recipient_id for external payouts (e.g. bank account) and reference_id for linking to recurring payments etc.
ALTER TABLE transactions
    ADD COLUMN IF NOT EXISTS external_recipient_id VARCHAR(255),
    ADD COLUMN IF NOT EXISTS reference_id UUID;

CREATE INDEX IF NOT EXISTS idx_transactions_reference_id ON transactions(reference_id) WHERE reference_id IS NOT NULL;
