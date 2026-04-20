# frozen_string_literal: true

require "test_helper"

class LedgerAccountTest < ActiveSupport::TestCase
  test "resolve creates account with expected attributes" do
    org = SecureRandom.uuid
    acct = LedgerAccount.resolve(
      organization_id: org,
      environment: "sandbox",
      external_account_id: "ext_#{SecureRandom.hex(4)}",
      currency: "USD",
      account_type: "liability"
    )
    assert_equal org, acct.organization_id
    assert_equal "sandbox", acct.environment
    assert_equal "liability", acct.account_type
  end

  test "current_balance applies liability sign when balance present" do
    org = SecureRandom.uuid
    acct = LedgerAccount.create!(
      organization_id: org,
      environment: "sandbox",
      external_account_id: "bal_#{SecureRandom.hex(4)}",
      account_type: "liability",
      currency: "USD"
    )
    AccountBalance.create!(
      organization_id: org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      balance_cents: 100,
      currency: "USD",
      last_updated_at: Time.current
    )
    acct.reload
    assert_equal(-100, acct.current_balance)
  end

  test "current_balance treats missing balance as zero for asset" do
    org = SecureRandom.uuid
    acct = LedgerAccount.create!(
      organization_id: org,
      environment: "production",
      external_account_id: "asset_#{SecureRandom.hex(4)}",
      account_type: "asset",
      currency: "USD"
    )
    assert_equal 0, acct.current_balance
  end

  test "current_balance covers equity income and expense with balance" do
    org = SecureRandom.uuid
    %w[equity income expense].each do |atype|
      acct = LedgerAccount.create!(
        organization_id: org,
        environment: "sandbox",
        external_account_id: "#{atype}_#{SecureRandom.hex(4)}",
        account_type: atype,
        currency: "USD"
      )
      AccountBalance.create!(
        organization_id: org,
        environment: "sandbox",
        ledger_account_id: acct.id,
        balance_cents: 50,
        currency: "USD",
        last_updated_at: Time.current
      )
      acct.reload
      expected = atype == "expense" ? 50 : -50
      assert_equal expected, acct.current_balance, "unexpected balance for #{atype}"
    end
  end

  test "current_balance raises for unsupported account_type reader" do
    acct = LedgerAccount.create!(
      organization_id: SecureRandom.uuid,
      environment: "sandbox",
      external_account_id: "bad_type_#{SecureRandom.hex(4)}",
      account_type: "asset",
      currency: "USD"
    )
    acct.define_singleton_method(:account_type) { "bogus" }
    err = assert_raises(ArgumentError) { acct.current_balance }
    assert_match(/Unsupported account_type/, err.message)
  end

  test "create_control_accounts returns three system accounts" do
    org = SecureRandom.uuid
    accounts = LedgerAccount.create_control_accounts(
      organization_id: org,
      environment: "sandbox",
      currency: "USD"
    )
    assert_equal %i[bank_clearing cash_control fee_income], accounts.keys.sort
    assert_equal "SYSTEM_CASH_CONTROL", accounts[:cash_control].external_account_id
  end
end
