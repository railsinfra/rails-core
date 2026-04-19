# frozen_string_literal: true

class AddSimpleConstraints < ActiveRecord::Migration[7.1]
  def change
    # Add constraint for account_type values if not exists
    execute <<-SQL
      DO $$
      BEGIN
        IF NOT EXISTS (
          SELECT 1 FROM pg_constraint 
          WHERE conname = 'check_account_type' 
          AND conrelid = 'ledger_accounts'::regclass
        ) THEN
          ALTER TABLE ledger_accounts 
          ADD CONSTRAINT check_account_type 
          CHECK (account_type IN ('asset', 'liability', 'equity', 'income', 'expense'));
        END IF;
      END $$;
    SQL

    # Add index for idempotency constraint if not exists
    add_index :ledger_transactions, [:organization_id, :environment, :idempotency_key], 
              unique: true, name: 'index_ledger_transactions_idempotency', if_not_exists: true
    
    # Add index for transaction_id on ledger_entries if not exists
    add_index :ledger_entries, :transaction_id, if_not_exists: true
  end

  def down
    # Remove account_type constraint
    execute 'ALTER TABLE ledger_accounts DROP CONSTRAINT IF EXISTS check_account_type'

    # Remove indexes
    remove_index :ledger_transactions, name: 'index_ledger_transactions_idempotency', if_exists: true
    remove_index :ledger_entries, :transaction_id, if_exists: true
  end
end
