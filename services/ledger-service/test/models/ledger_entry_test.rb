# frozen_string_literal: true

require "test_helper"

class LedgerEntryTest < ActiveSupport::TestCase
  test "rejects invalid entry_type" do
    org = SecureRandom.uuid
    acct = LedgerAccount.create!(
      organization_id: org,
      environment: "sandbox",
      external_account_id: "le_#{SecureRandom.hex(4)}",
      account_type: "asset",
      currency: "USD"
    )
    tx = LedgerTransaction.create!(
      organization_id: org,
      environment: "sandbox",
      external_transaction_id: SecureRandom.uuid,
      status: "posted",
      idempotency_key: SecureRandom.uuid
    )
    entry = LedgerEntry.new(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      transaction_id: tx.id,
      entry_type: "bogus",
      amount: 1,
      currency: "USD"
    )
    assert_not entry.valid?
    assert_includes entry.errors[:entry_type], "is not included in the list"
  end
end
