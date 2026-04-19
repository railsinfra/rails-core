require "test_helper"

class LedgerPosterTest < ActiveSupport::TestCase
  test "post_deposit creates balanced entries in the same environment" do
    organization_id = SecureRandom.uuid
    transaction = LedgerPoster.post_deposit(
      organization_id: organization_id,
      environment: "sandbox",
      destination_external_account_id: "acct_123",
      amount: 10_000,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )

    assert_equal "posted", transaction.status
    assert_equal "sandbox", transaction.environment
    assert_equal organization_id, transaction.organization_id

    entries = LedgerEntry.where(transaction_id: transaction.id)
    assert_equal 2, entries.count
    assert_equal ["credit", "debit"], entries.pluck(:entry_type).sort
    assert_equal ["sandbox"], entries.pluck(:environment).uniq

    debit_total = entries.select { |entry| entry.entry_type == "debit" }.sum(&:amount)
    credit_total = entries.select { |entry| entry.entry_type == "credit" }.sum(&:amount)
    assert_equal debit_total, credit_total
  end
end
