class CreateLedgerAccounts < ActiveRecord::Migration[7.1]
  def change
    create_table :ledger_accounts, id: :uuid do |t|
      t.uuid :organization_id, null: false
      t.string :environment, null: false, limit: 20
      t.uuid :external_account_id, null: false
      t.string :account_type, null: false
      t.string :currency, null: false
      t.timestamps null: false

      t.index [:organization_id, :environment, :external_account_id, :currency], 
              name: "index_ledger_accounts_on_org_env_external_currency", 
              unique: true
      t.index [:organization_id, :environment], name: "index_ledger_accounts_on_org_env"
    end
  end
end
