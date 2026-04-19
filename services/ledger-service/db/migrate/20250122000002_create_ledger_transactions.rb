class CreateLedgerTransactions < ActiveRecord::Migration[7.1]
  def change
    create_table :ledger_transactions, id: :uuid do |t|
      t.uuid :organization_id, null: false
      t.string :environment, null: false, limit: 20
      t.uuid :external_transaction_id, null: false
      t.string :status, null: false, default: 'pending'
      t.text :failure_reason
      t.string :idempotency_key, null: false
      t.timestamps null: false

      t.index [:organization_id, :environment, :idempotency_key], 
              name: "index_ledger_transactions_on_org_env_idempotency", 
              unique: true
      t.index [:organization_id, :environment, :external_transaction_id], 
              name: "index_ledger_transactions_on_org_env_external"
      t.index [:organization_id, :environment, :status], 
              name: "index_ledger_transactions_on_org_env_status"
    end
  end
end
