# frozen_string_literal: true

require "test_helper"

class AccountBalanceTest < ActiveSupport::TestCase
  test "update_balance applies debit and credit" do
    org = SecureRandom.uuid
    acct = LedgerAccount.create!(
      organization_id: org,
      environment: "sandbox",
      external_account_id: "ab_#{SecureRandom.hex(4)}",
      account_type: "asset",
      currency: "USD"
    )

    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      amount_cents: 50,
      currency: "USD",
      entry_type: "debit"
    )
    assert_equal 50, AccountBalance.get_balance(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id
    )

    AccountBalance.update_balance!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      amount_cents: 20,
      currency: "USD",
      entry_type: "credit"
    )
    assert_equal 30, AccountBalance.get_balance(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id
    )
  end

  test "update_balance rejects invalid entry type" do
    org = SecureRandom.uuid
    acct = LedgerAccount.create!(
      organization_id: org,
      environment: "sandbox",
      external_account_id: "ab2_#{SecureRandom.hex(4)}",
      account_type: "asset",
      currency: "USD"
    )

    assert_raises(ArgumentError) do
      AccountBalance.update_balance!(
        organization_id: org,
        environment: "sandbox",
        ledger_account_id: acct.id,
        amount_cents: 1,
        currency: "USD",
        entry_type: "bogus"
      )
    end
  end
end
