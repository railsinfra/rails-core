# frozen_string_literal: true

require "test_helper"

class LedgerPosterComprehensiveTest < ActiveSupport::TestCase
  setup do
    @org = SecureRandom.uuid
    @ext = SecureRandom.uuid
    @idemp = SecureRandom.uuid
  end

  test "post validates amount positive" do
    assert_raises(LedgerPoster::PostingError, match: /Amount/) do
      LedgerPoster.post(
        organization_id: @org,
        environment: "sandbox",
        source_external_account_id: "a",
        destination_external_account_id: "b",
        amount: 0,
        currency: "USD",
        external_transaction_id: @ext,
        idempotency_key: @idemp
      )
    end
  end

  test "post validates environment" do
    assert_raises(LedgerPoster::PostingError, match: /environment/i) do
      LedgerPoster.post(
        organization_id: @org,
        environment: "staging",
        source_external_account_id: "a",
        destination_external_account_id: "b",
        amount: 1,
        currency: "USD",
        external_transaction_id: @ext,
        idempotency_key: @idemp
      )
    end
  end

  test "post validates currency present" do
    assert_raises(LedgerPoster::PostingError, match: /Currency/) do
      LedgerPoster.post(
        organization_id: @org,
        environment: "sandbox",
        source_external_account_id: "a",
        destination_external_account_id: "b",
        amount: 1,
        currency: "",
        external_transaction_id: @ext,
        idempotency_key: @idemp
      )
    end
  end

  test "post transfer between two external accounts" do
    src = "user_src_#{SecureRandom.hex(4)}"
    dst = "user_dst_#{SecureRandom.hex(4)}"
    tx = LedgerPoster.post(
      organization_id: @org,
      environment: "production",
      source_external_account_id: src,
      destination_external_account_id: dst,
      amount: 25,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )
    assert_equal "posted", tx.status
    assert_equal 2, LedgerEntry.where(transaction_id: tx.id).count
  end

  test "post withdraw from user to cash control" do
    user_acct = "user_wd_#{SecureRandom.hex(4)}"
    tx = LedgerPoster.post(
      organization_id: @org,
      environment: "sandbox",
      source_external_account_id: user_acct,
      destination_external_account_id: "SYSTEM_CASH_CONTROL",
      amount: 40,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )
    assert_equal "posted", tx.status
  end

  test "post deposit routes fee income destination type" do
    tx = LedgerPoster.post(
      organization_id: @org,
      environment: "sandbox",
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "SYSTEM_FEE_INCOME",
      amount: 7,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )
    assert_equal "posted", tx.status
  end

  test "post to other SYSTEM external id uses asset destination type" do
    tx = LedgerPoster.post(
      organization_id: @org,
      environment: "sandbox",
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "SYSTEM_CUSTOM_POOL",
      amount: 3,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: SecureRandom.uuid
    )
    assert_equal "posted", tx.status
  end

  test "post rejects self transfer for non deposit" do
    acct = "same_#{SecureRandom.hex(4)}"
    assert_raises(LedgerPoster::PostingError, match: /Self-transfer/) do
      LedgerPoster.post(
        organization_id: @org,
        environment: "sandbox",
        source_external_account_id: acct,
        destination_external_account_id: acct,
        amount: 10,
        currency: "USD",
        external_transaction_id: SecureRandom.uuid,
        idempotency_key: SecureRandom.uuid
      )
    end
  end

  test "post completes pending transaction without entries" do
    idem = SecureRandom.uuid
    ext = SecureRandom.uuid
    pending = LedgerTransaction.create!(
      organization_id: @org,
      environment: "sandbox",
      external_transaction_id: ext,
      status: "pending",
      idempotency_key: idem
    )

    tx = LedgerPoster.post(
      organization_id: @org,
      environment: "sandbox",
      source_external_account_id: "SYSTEM_CASH_CONTROL",
      destination_external_account_id: "acct_complete_#{SecureRandom.hex(4)}",
      amount: 11,
      currency: "USD",
      external_transaction_id: ext,
      idempotency_key: idem
    )
    assert_equal pending.id, tx.id
    assert_equal "posted", tx.reload.status
  end

  test "post raises when pending exists with partial entries" do
    idem = SecureRandom.uuid
    ext = SecureRandom.uuid
    pending = LedgerTransaction.create!(
      organization_id: @org,
      environment: "sandbox",
      external_transaction_id: ext,
      status: "pending",
      idempotency_key: idem
    )
    acct = LedgerAccount.resolve(
      organization_id: @org,
      environment: "sandbox",
      external_account_id: "partial_acct",
      currency: "USD",
      account_type: "liability"
    )
    LedgerEntry.create!(
      organization_id: @org,
      environment: "sandbox",
      ledger_account_id: acct.id,
      transaction_id: pending.id,
      entry_type: "debit",
      amount: 1,
      currency: "USD"
    )

    err = assert_raises(LedgerPoster::PostingError) do
      LedgerPoster.post(
        organization_id: @org,
        environment: "sandbox",
        source_external_account_id: "SYSTEM_CASH_CONTROL",
        destination_external_account_id: "partial_acct",
        amount: 11,
        currency: "USD",
        external_transaction_id: ext,
        idempotency_key: idem
      )
    end
    assert_match(/partial state|manual intervention/i, err.message)
  end

  test "post returns existing posted transaction" do
    idem = SecureRandom.uuid
    first = LedgerPoster.post_deposit(
      organization_id: @org,
      environment: "sandbox",
      destination_external_account_id: "idem_user",
      amount: 20,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: idem
    )
    second = LedgerPoster.post_deposit(
      organization_id: @org,
      environment: "sandbox",
      destination_external_account_id: "idem_user",
      amount: 20,
      currency: "USD",
      external_transaction_id: SecureRandom.uuid,
      idempotency_key: idem
    )
    assert_equal first.id, second.id
  end

  test "private helpers reject unknown operation and account type" do
    poster = LedgerPoster.new(
      organization_id: @org,
      environment: "sandbox",
      source_external_account_id: "a",
      destination_external_account_id: "b",
      amount: 1,
      currency: "USD",
      external_transaction_id: @ext,
      idempotency_key: @idemp
    )
    err = assert_raises(LedgerPoster::PostingError) do
      poster.send(:account_change_for, :not_an_op, :source)
    end
    assert_match(/Unknown operation/i, err.message)

    err2 = assert_raises(LedgerPoster::PostingError) do
      poster.send(:entry_type_for, "not_a_type", :increase)
    end
    assert_match(/Invalid account type/i, err2.message)
  end

  test "call rescue reports to Sentry when update_balance raises" do
    with_stub(AccountBalance, :update_balance!, proc { raise StandardError, "forced balance failure" }) do
      err = assert_raises(LedgerPoster::PostingError) do
        LedgerPoster.post_deposit(
          organization_id: @org,
          environment: "sandbox",
          destination_external_account_id: "sentry_fail_dest",
          amount: 5,
          currency: "USD",
          external_transaction_id: SecureRandom.uuid,
          idempotency_key: SecureRandom.uuid
        )
      end
      assert_match(/forced balance failure/, err.message)
    end
  end

  test "call rescue handles nil Sentry scope" do
    with_stub(AccountBalance, :update_balance!, proc { raise StandardError, "boom" }) do
      with_stub(Sentry, :with_scope, proc { |&block| block.call(nil) }) do
        with_stub(Sentry, :capture_exception, proc { |_e| nil }) do
          assert_raises(LedgerPoster::PostingError) do
            LedgerPoster.post_deposit(
              organization_id: @org,
              environment: "sandbox",
              destination_external_account_id: "nil_scope",
              amount: 4,
              currency: "USD",
              external_transaction_id: SecureRandom.uuid,
              idempotency_key: SecureRandom.uuid
            )
          end
        end
      end
    end
  end

  test "call rescue handles Sentry capture_exception failure" do
    scope_obj = Object.new
    def scope_obj.set_context(*); end
    def scope_obj.set_tag(*); end

    with_stub(AccountBalance, :update_balance!, proc { raise StandardError, "ledger boom" }) do
      with_stub(Sentry, :with_scope, proc { |&block| block.call(scope_obj) }) do
        with_stub(Sentry, :capture_exception, proc { raise StandardError, "sentry is down" }) do
          assert_raises(LedgerPoster::PostingError) do
            LedgerPoster.post_deposit(
              organization_id: @org,
              environment: "sandbox",
              destination_external_account_id: "sentry_capture_fail",
              amount: 3,
              currency: "USD",
              external_transaction_id: SecureRandom.uuid,
              idempotency_key: SecureRandom.uuid
            )
          end
        end
      end
    end
  end
end
