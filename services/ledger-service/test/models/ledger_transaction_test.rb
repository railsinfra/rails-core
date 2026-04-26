# frozen_string_literal: true

require "test_helper"

class LedgerTransactionTest < ActiveSupport::TestCase
  test "find_existing returns nil when no row" do
    assert_nil LedgerTransaction.find_existing(
      organization_id: SecureRandom.uuid,
      environment: "sandbox",
      idempotency_key: SecureRandom.uuid
    )
  end

  test "mark_as_posted clears failure_reason" do
    org = SecureRandom.uuid
    tx = LedgerTransaction.create!(
      organization_id: org,
      environment: "sandbox",
      external_transaction_id: SecureRandom.uuid,
      status: "pending",
      idempotency_key: SecureRandom.uuid,
      failure_reason: "prior"
    )
    tx.mark_as_posted!
    tx.reload
    assert_equal "posted", tx.status
    assert_nil tx.failure_reason
  end

  test "after_commit logs when audit append raises" do
    logged = []
    with_stub(Rails.logger, :error, proc { |*args| logged << args.join }) do
      with_stub(AuditAppend, :emit_ledger_transaction_posted, proc { |_| raise StandardError, "audit boom" }) do
        tx = LedgerTransaction.create!(
          organization_id: SecureRandom.uuid,
          environment: "sandbox",
          external_transaction_id: SecureRandom.uuid,
          status: "pending",
          idempotency_key: SecureRandom.uuid
        )
        tx.mark_as_posted!
      end
    end
    assert logged.any? { |m| m.include?("after_commit error") && m.include?("audit boom") }
  end

  test "mark_as_posted triggers audit append hook" do
    calls = []
    meta = AuditAppend.singleton_class
    original = meta.instance_method(:emit_ledger_transaction_posted)
    begin
      meta.define_method(:emit_ledger_transaction_posted) { |arg| calls << arg }

      org = SecureRandom.uuid
      tx = LedgerTransaction.create!(
        organization_id: org,
        environment: "sandbox",
        external_transaction_id: SecureRandom.uuid,
        status: "pending",
        idempotency_key: SecureRandom.uuid
      )
      tx.mark_as_posted!
      assert_equal 1, calls.size
      assert_equal tx.id, calls.first.id
    ensure
      meta.define_method(:emit_ledger_transaction_posted, original)
    end
  end

  test "mark_as_failed does not trigger audit append hook" do
    calls = []
    meta = AuditAppend.singleton_class
    original = meta.instance_method(:emit_ledger_transaction_posted)
    begin
      meta.define_method(:emit_ledger_transaction_posted) { |arg| calls << arg }

      org = SecureRandom.uuid
      tx = LedgerTransaction.create!(
        organization_id: org,
        environment: "sandbox",
        external_transaction_id: SecureRandom.uuid,
        status: "pending",
        idempotency_key: SecureRandom.uuid
      )
      tx.mark_as_failed!(reason: "nope")
      assert_empty calls
    ensure
      meta.define_method(:emit_ledger_transaction_posted, original)
    end
  end

  test "mark_as_failed sets status and reason" do
    org = SecureRandom.uuid
    tx = LedgerTransaction.create!(
      organization_id: org,
      environment: "production",
      external_transaction_id: SecureRandom.uuid,
      status: "pending",
      idempotency_key: SecureRandom.uuid
    )
    tx.mark_as_failed!(reason: "boom")
    tx.reload
    assert_equal "failed", tx.status
    assert_equal "boom", tx.failure_reason
  end

  test "validations reject invalid environment" do
    tx = LedgerTransaction.new(
      organization_id: SecureRandom.uuid,
      environment: "staging",
      external_transaction_id: SecureRandom.uuid,
      status: "pending",
      idempotency_key: SecureRandom.uuid
    )
    assert_not tx.valid?
    assert_includes tx.errors[:environment], "is not included in the list"
  end
end
