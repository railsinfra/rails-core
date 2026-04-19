# frozen_string_literal: true

class CreateAccountBalancesIfMissing < ActiveRecord::Migration[7.1]
  def up
    # Only create if table doesn't exist (fixes schema inconsistency)
    unless table_exists?(:account_balances)
      create_table :account_balances do |t|
        t.uuid :organization_id, null: false
        t.string :environment, null: false, limit: 20
        t.uuid :ledger_account_id, null: false
        t.bigint :balance_cents, null: false, default: 0
        t.string :currency, null: false, limit: 3
        t.datetime :last_updated_at, null: false, precision: 6
        t.timestamps
      end

      add_index :account_balances, [:organization_id, :environment, :ledger_account_id], 
                unique: true, name: 'index_account_balances_unique'
      add_index :account_balances, :ledger_account_id
      add_index :account_balances, [:organization_id, :environment]
    end
  end

  def down
    drop_table :account_balances if table_exists?(:account_balances)
  end
end
