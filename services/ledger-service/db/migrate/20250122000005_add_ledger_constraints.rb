# frozen_string_literal: true

class AddLedgerConstraints < ActiveRecord::Migration[7.1]
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

    # Create account_balances table if not exists
    create_table :account_balances, if_not_exists: true do |t|
      t.uuid :organization_id, null: false
      t.string :environment, null: false, limit: 20
      t.uuid :ledger_account_id, null: false
      t.bigint :balance_cents, null: false, default: 0
      t.string :currency, null: false, limit: 3
      t.datetime :last_updated_at, null: false, precision: 6
      t.timestamps
    end

    # Add unique constraint for account_balances if not exists
    add_index :account_balances, [:organization_id, :environment, :ledger_account_id], 
              unique: true, name: 'index_account_balances_unique', if_not_exists: true
    add_index :account_balances, :ledger_account_id, if_not_exists: true
    add_index :account_balances, [:organization_id, :environment], if_not_exists: true

    # Initialize balances for existing accounts if not already done
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
      LEFT JOIN account_balances ab ON ab.ledger_account_id = la.id
      WHERE ab.ledger_account_id IS NULL
      GROUP BY la.id, la.organization_id, la.environment, la.currency
      ON CONFLICT (organization_id, environment, ledger_account_id) DO NOTHING;
    SQL

    # Make ledger entries immutable - no updates or deletes allowed
    execute <<-SQL
      DO $$
      BEGIN
        IF NOT EXISTS (
          SELECT 1 FROM pg_rule 
          WHERE rulename = 'prevent_ledger_entry_updates'
        ) THEN
          CREATE RULE prevent_ledger_entry_updates AS
          ON UPDATE TO ledger_entries
          DO INSTEAD NOTHING;
        END IF;

        IF NOT EXISTS (
          SELECT 1 FROM pg_rule 
          WHERE rulename = 'prevent_ledger_entry_deletes'
        ) THEN
          CREATE RULE prevent_ledger_entry_deletes AS
          ON DELETE TO ledger_entries
          DO INSTEAD NOTHING;
        END IF;
      END $$;
    SQL

    # Add constraint to ensure exactly 2 entries per transaction
    execute <<-SQL
      DO $$
      BEGIN
        IF NOT EXISTS (
          SELECT 1 FROM pg_constraint 
          WHERE conname = 'check_transaction_entry_count' 
          AND conrelid = 'ledger_transactions'::regclass
        ) THEN
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
        END IF;
      END $$;
    SQL

    # Add constraint to ensure balanced debits and credits
    execute <<-SQL
      DO $$
      BEGIN
        IF NOT EXISTS (
          SELECT 1 FROM pg_constraint 
          WHERE conname = 'check_transaction_balanced' 
          AND conrelid = 'ledger_transactions'::regclass
        ) THEN
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
    # Remove constraints and rules
    execute 'DROP RULE IF EXISTS prevent_ledger_entry_updates ON ledger_entries'

    execute 'DROP RULE IF EXISTS prevent_ledger_entry_deletes ON ledger_entries'

    execute 'ALTER TABLE ledger_transactions DROP CONSTRAINT IF EXISTS check_transaction_entry_count'

    execute 'ALTER TABLE ledger_transactions DROP CONSTRAINT IF EXISTS check_transaction_balanced'

    # Remove indexes
    remove_index :account_balances, name: 'index_account_balances_unique', if_exists: true
    remove_index :account_balances, :ledger_account_id, if_exists: true
    remove_index :account_balances, [:organization_id, :environment], if_exists: true
    remove_index :ledger_transactions, name: 'index_ledger_transactions_idempotency', if_exists: true
    remove_index :ledger_entries, :transaction_id, if_exists: true

    # Drop account_balances table
    drop_table :account_balances, if_exists: true

    # Remove account_type constraint
    execute 'ALTER TABLE ledger_accounts DROP CONSTRAINT IF EXISTS check_account_type'
  end
end
