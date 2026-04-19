# frozen_string_literal: true

class AddAccountTypesAndBalances < ActiveRecord::Migration[7.1]
  def change
    # Add proper account_type enum to ledger_accounts
    add_column :ledger_accounts, :account_type, :string, null: false, default: 'liability'
    
    # Update existing accounts to have proper types
    # For POC, assume existing accounts are customer deposits (liabilities)
    execute <<-SQL
      UPDATE ledger_accounts 
      SET account_type = 'liability' 
      WHERE account_type = 'asset';
    SQL

    # Add constraint for account_type values
    execute <<-SQL
      ALTER TABLE ledger_accounts 
      ADD CONSTRAINT check_account_type 
      CHECK (account_type IN ('asset', 'liability', 'equity', 'income', 'expense'));
    SQL

    # Create account_balances table for transactional balance updates
    create_table :account_balances do |t|
      t.uuid :organization_id, null: false
      t.string :environment, null: false, limit: 20
      t.uuid :ledger_account_id, null: false
      t.bigint :balance_cents, null: false, default: 0
      t.string :currency, null: false, limit: 3
      t.datetime :last_updated_at, null: false, precision: 6
      t.timestamps
    end

    # Add unique constraint for account_balances
    add_index :account_balances, [:organization_id, :environment, :ledger_account_id], 
              unique: true, name: 'index_account_balances_unique'
    add_index :account_balances, :ledger_account_id
    add_index :account_balances, [:organization_id, :environment]

    # Initialize balances for existing accounts
    execute <<-SQL
      INSERT INTO account_balances (
        organization_id, 
        environment, 
        ledger_account_id, 
        balance_cents, 
        currency, 
        last_updated_at,
        created_at,
        updated_at
      )
      SELECT 
        la.organization_id,
        la.environment,
        la.id,
        COALESCE(
          SUM(CASE WHEN le.entry_type = 'debit' THEN le.amount ELSE -le.amount END), 
          0
        ) as balance_cents,
        la.currency,
        NOW(),
        NOW(),
        NOW()
      FROM ledger_accounts la
      LEFT JOIN ledger_entries le ON le.ledger_account_id = la.id
      GROUP BY la.id, la.organization_id, la.environment, la.currency;
    SQL

    # Make ledger entries immutable - no updates or deletes allowed
    execute <<-SQL
      CREATE RULE prevent_ledger_entry_updates AS
      ON UPDATE TO ledger_entries
      DO INSTEAD NOTHING;
    SQL

    execute <<-SQL
      CREATE RULE prevent_ledger_entry_deletes AS
      ON DELETE TO ledger_entries
      DO INSTEAD NOTHING;
    SQL

    # Add constraint to ensure exactly 2 entries per transaction
    execute <<-SQL
      ALTER TABLE ledger_transactions 
      ADD CONSTRAINT check_transaction_entry_count 
      CHECK (
        id IN (
          SELECT transaction_id 
          FROM ledger_entries 
          WHERE transaction_id = ledger_transactions.id 
          GROUP BY transaction_id 
          HAVING COUNT(*) = 2
        )
      );
    SQL

    # Add constraint to ensure balanced debits and credits
    execute <<-SQL
      ALTER TABLE ledger_transactions 
      ADD CONSTRAINT check_transaction_balanced 
      CHECK (
        id IN (
          SELECT transaction_id 
          FROM ledger_entries 
          WHERE transaction_id = ledger_transactions.id 
          GROUP BY transaction_id 
          HAVING 
            SUM(CASE WHEN entry_type = 'debit' THEN amount ELSE 0 END) = 
            SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END)
        )
      );
    SQL

    # Add index for idempotency constraint
    add_index :ledger_transactions, [:organization_id, :environment, :idempotency_key], 
              unique: true, name: 'index_ledger_transactions_idempotency'
    
    # Add index for transaction_id on ledger_entries
    add_index :ledger_entries, :transaction_id
  end

  def down
    # Remove constraints and rules
    execute <<-SQL
      DROP RULE IF EXISTS prevent_ledger_entry_updates ON ledger_entries;
    SQL

    execute <<-SQL
      DROP RULE IF EXISTS prevent_ledger_entry_deletes ON ledger_entries;
    SQL

    execute <<-SQL
      ALTER TABLE ledger_transactions DROP CONSTRAINT IF EXISTS check_transaction_entry_count;
    SQL

    execute <<-SQL
      ALTER TABLE ledger_transactions DROP CONSTRAINT IF EXISTS check_transaction_balanced;
    SQL

    # Remove indexes
    remove_index :account_balances, name: 'index_account_balances_unique'
    remove_index :account_balances, :ledger_account_id
    remove_index :account_balances, [:organization_id, :environment]
    remove_index :ledger_transactions, name: 'index_ledger_transactions_idempotency'
    remove_index :ledger_entries, :transaction_id

    # Drop account_balances table
    drop_table :account_balances

    # Remove account_type constraint and column
    execute <<-SQL
      ALTER TABLE ledger_accounts DROP CONSTRAINT IF EXISTS check_account_type;
    SQL

    remove_column :ledger_accounts, :account_type
  end
end
