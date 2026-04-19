-- Create recurring_payments table
CREATE TABLE IF NOT EXISTS recurring_payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    recipient_account_id UUID REFERENCES accounts(id) ON DELETE SET NULL,
    external_recipient_id VARCHAR(255),
    recipient_type VARCHAR(20) NOT NULL CHECK (recipient_type IN ('internal', 'external')),
    amount DECIMAL(19, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    frequency VARCHAR(20) NOT NULL CHECK (frequency IN ('daily', 'weekly', 'biweekly', 'monthly', 'quarterly', 'yearly')),
    trigger_condition JSONB,
    next_execution_date DATE NOT NULL,
    last_execution_date DATE,
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'completed', 'cancelled')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_recurring_payments_account_id ON recurring_payments(account_id);
CREATE INDEX IF NOT EXISTS idx_recurring_payments_next_execution_date ON recurring_payments(next_execution_date);
CREATE INDEX IF NOT EXISTS idx_recurring_payments_status ON recurring_payments(status);
