# This file is auto-generated from the current state of the database. Instead
# of editing this file, please use the migrations feature of Active Record to
# incrementally modify your database, and then regenerate this schema definition.
#
# This file is the source Rails uses to define your schema when running `bin/rails
# db:schema:load`. When creating a new database, `bin/rails db:schema:load` tends to
# be faster and is potentially less error prone than running all of your
# migrations from scratch. Old migrations may fail to apply correctly if those
# migrations use external dependencies or application code.
#
# It's strongly recommended that you check this file into your version control system.

ActiveRecord::Schema[7.1].define(version: 2026_01_27_000001) do
  # These are extensions that must be enabled in order to support this database
  enable_extension "plpgsql"

  create_table "account_balances", force: :cascade do |t|
    t.uuid "organization_id", null: false
    t.string "environment", limit: 20, null: false
    t.uuid "ledger_account_id", null: false
    t.bigint "balance_cents", default: 0, null: false
    t.string "currency", limit: 3, null: false
    t.datetime "last_updated_at", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["environment", "organization_id", "ledger_account_id"], name: "idx_account_balances_env_org_account"
    t.index ["ledger_account_id"], name: "index_account_balances_on_ledger_account_id"
    t.index ["organization_id", "environment", "ledger_account_id"], name: "index_account_balances_unique", unique: true
    t.index ["organization_id", "environment"], name: "index_account_balances_on_organization_id_and_environment"
  end

  create_table "ledger_accounts", id: :uuid, default: -> { "gen_random_uuid()" }, force: :cascade do |t|
    t.uuid "organization_id", null: false
    t.string "environment", limit: 20, null: false
    t.string "external_account_id", null: false
    t.string "account_type", null: false
    t.string "currency", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["environment", "organization_id", "external_account_id", "currency"], name: "idx_ledger_accounts_env_org_external_currency"
    t.index ["organization_id", "environment", "external_account_id", "currency"], name: "index_ledger_accounts_on_org_env_external_currency", unique: true
    t.index ["organization_id", "environment"], name: "index_ledger_accounts_on_org_env"
    t.check_constraint "account_type::text = ANY (ARRAY['asset'::character varying, 'liability'::character varying, 'equity'::character varying, 'income'::character varying, 'expense'::character varying]::text[])", name: "check_account_type"
  end

  create_table "ledger_entries", id: :uuid, default: -> { "gen_random_uuid()" }, force: :cascade do |t|
    t.uuid "organization_id", null: false
    t.string "environment", limit: 20, null: false
    t.uuid "ledger_account_id", null: false
    t.uuid "transaction_id", null: false
    t.string "entry_type", null: false
    t.bigint "amount", null: false
    t.string "currency", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["environment", "organization_id", "ledger_account_id", "created_at"], name: "idx_ledger_entries_env_org_account_created"
    t.index ["ledger_account_id"], name: "index_ledger_entries_on_account"
    t.index ["organization_id", "environment", "ledger_account_id"], name: "index_ledger_entries_on_org_env_account"
    t.index ["organization_id", "environment", "transaction_id"], name: "index_ledger_entries_on_org_env_transaction"
    t.index ["transaction_id"], name: "index_ledger_entries_on_transaction_id"
  end

  create_table "ledger_transactions", id: :uuid, default: -> { "gen_random_uuid()" }, force: :cascade do |t|
    t.uuid "organization_id", null: false
    t.string "environment", limit: 20, null: false
    t.uuid "external_transaction_id", null: false
    t.string "status", default: "pending", null: false
    t.text "failure_reason"
    t.string "idempotency_key", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["environment", "organization_id", "created_at"], name: "idx_ledger_transactions_env_org_created"
    t.index ["organization_id", "environment", "external_transaction_id"], name: "index_ledger_transactions_on_org_env_external"
    t.index ["organization_id", "environment", "idempotency_key"], name: "index_ledger_transactions_idempotency", unique: true
    t.index ["organization_id", "environment", "idempotency_key"], name: "index_ledger_transactions_on_org_env_idempotency", unique: true
    t.index ["organization_id", "environment", "status"], name: "index_ledger_transactions_on_org_env_status"
  end

end
