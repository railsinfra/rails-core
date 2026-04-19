-- Create fixed_savings_plans table
CREATE TABLE IF NOT EXISTS fixed_savings_plans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    plan_type VARCHAR(20) NOT NULL CHECK (plan_type IN ('auto_withdraw', 'date_locked')),
    initial_amount DECIMAL(19, 4) NOT NULL,
    current_balance DECIMAL(19, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    monthly_withdraw_amount DECIMAL(19, 4),
    next_withdraw_date DATE,
    unlock_date DATE,
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'cancelled')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_fixed_savings_plans_account_id ON fixed_savings_plans(account_id);
CREATE INDEX IF NOT EXISTS idx_fixed_savings_plans_unlock_date ON fixed_savings_plans(unlock_date);
CREATE INDEX IF NOT EXISTS idx_fixed_savings_plans_next_withdraw_date ON fixed_savings_plans(next_withdraw_date);
CREATE INDEX IF NOT EXISTS idx_fixed_savings_plans_status ON fixed_savings_plans(status);
