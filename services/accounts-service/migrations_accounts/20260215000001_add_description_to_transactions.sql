-- Add description column to transactions for user-provided notes (deposit, withdraw, transfer).
ALTER TABLE transactions
    ADD COLUMN IF NOT EXISTS description TEXT;
