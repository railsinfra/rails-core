-- Add admin_user_id and user_role columns to accounts table
-- These columns are needed for customer accounts that have an admin relationship
-- Note: admin_user_id references users in the Users service database (no FK constraint)

ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS admin_user_id UUID,
    ADD COLUMN IF NOT EXISTS user_role VARCHAR(20) CHECK (user_role IN ('ADMIN', 'CUSTOMER', 'SERVICE'));

-- Create index for admin_user_id lookups
CREATE INDEX IF NOT EXISTS idx_accounts_admin_user_id ON accounts(admin_user_id)
WHERE admin_user_id IS NOT NULL;

-- Create index for user_role filtering
CREATE INDEX IF NOT EXISTS idx_accounts_user_role ON accounts(user_role)
WHERE user_role IS NOT NULL;
