class CreateLedgerEntries < ActiveRecord::Migration[7.1]
  def change
    create_table :ledger_entries, id: :uuid do |t|
      t.uuid :organization_id, null: false
      t.string :environment, null: false, limit: 20
      t.uuid :ledger_account_id, null: false
      t.uuid :transaction_id, null: false
      t.string :entry_type, null: false
      t.bigint :amount, null: false
      t.string :currency, null: false
      t.timestamps null: false

      t.index [:organization_id, :environment, :transaction_id], 
              name: "index_ledger_entries_on_org_env_transaction"
      t.index [:ledger_account_id], name: "index_ledger_entries_on_account"
      t.index [:organization_id, :environment, :ledger_account_id], 
              name: "index_ledger_entries_on_org_env_account"
    end
  end
end
