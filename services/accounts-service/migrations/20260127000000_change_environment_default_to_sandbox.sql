-- Change default environment from 'production' to 'sandbox' for safety
-- This prevents accidental production account creation when environment is not specified
-- Production accounts must be explicitly requested

ALTER TABLE accounts
    ALTER COLUMN environment SET DEFAULT 'sandbox';

-- Note: This does NOT change existing records, only affects new inserts without explicit environment
