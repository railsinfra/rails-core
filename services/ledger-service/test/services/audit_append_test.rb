# frozen_string_literal: true

require "test_helper"

class AuditAppendTest < ActiveSupport::TestCase
  include TestStubbing

  def setup
    @audit_url_was = ENV["AUDIT_GRPC_URL"]
    @timeout_ms_was = ENV["AUDIT_APPEND_TIMEOUT_MS"]
    @factory_was = AuditAppend.audit_stub_factory
  end

  def teardown
    restore_env("AUDIT_GRPC_URL", @audit_url_was)
    restore_env("AUDIT_APPEND_TIMEOUT_MS", @timeout_ms_was)
    AuditAppend.audit_stub_factory = @factory_was
  end

  def restore_env(key, was)
    if was.nil?
      ENV.delete(key)
    else
      ENV[key] = was
    end
  end

  test "append_timeout_sec uses default when unset" do
    ENV.delete("AUDIT_APPEND_TIMEOUT_MS")
    assert_in_delta 5.0, AuditAppend.append_timeout_sec, 0.001
  end

  test "append_timeout_sec parses custom milliseconds" do
    ENV["AUDIT_APPEND_TIMEOUT_MS"] = "2500"
    assert_in_delta 2.5, AuditAppend.append_timeout_sec, 0.001
  end

  test "append_timeout_sec clamps non positive to default" do
    ENV["AUDIT_APPEND_TIMEOUT_MS"] = "0"
    assert_in_delta 5.0, AuditAppend.append_timeout_sec, 0.001
  end

  test "append_timeout_sec clamps above max" do
    ENV["AUDIT_APPEND_TIMEOUT_MS"] = "999_000"
    assert_in_delta 120.0, AuditAppend.append_timeout_sec, 0.001
  end

  test "emit_ledger_transaction_posted no-ops when audit url missing" do
    ENV.delete("AUDIT_GRPC_URL")
    tx = sample_tx
    assert_nothing_raised { AuditAppend.emit_ledger_transaction_posted(tx) }
  end

  test "emit_ledger_transaction_posted calls append when stub succeeds" do
    ENV["AUDIT_GRPC_URL"] = "127.0.0.1:59999"
    tx = sample_tx
    calls = []
    fake = Object.new
    fake.define_singleton_method(:append_audit_event) do |req, deadline:|
      calls << [req, deadline]
    end
    AuditAppend.audit_stub_factory = proc { |_target, _timeout| fake }

    AuditAppend.emit_ledger_transaction_posted(tx)

    assert_equal 1, calls.size
    assert calls.first.first.is_a?(Rails::Core::Audit::V1::AppendAuditEventRequest)
  end

  test "emit_ledger_transaction_posted logs and reports when append raises" do
    ENV["AUDIT_GRPC_URL"] = "127.0.0.1:59998"
    tx = sample_tx
    fake = Object.new
    fake.define_singleton_method(:append_audit_event) { |_req, deadline:| raise GRPC::Unavailable, "down" }
    AuditAppend.audit_stub_factory = proc { |_target, _timeout| fake }

    reported = []
    orig = AuditAppend.method(:report_sentry_failure)
    AuditAppend.define_singleton_method(:report_sentry_failure) do |e, oid, lid|
      reported << [e, oid, lid]
    end
    begin
      AuditAppend.emit_ledger_transaction_posted(tx)
    ensure
      AuditAppend.define_singleton_method(:report_sentry_failure, orig)
    end

    assert_equal 1, reported.size
    assert_instance_of GRPC::Unavailable, reported.first[0]
    assert_equal tx.organization_id, reported.first[1]
    assert_equal tx.id, reported.first[2]
  end

  test "start_sentry_transaction returns nil when start_transaction missing" do
    with_sentry_start_transaction_removed do
      assert_nil AuditAppend.start_sentry_transaction
    end
  end

  test "start_sentry_transaction returns nil when start_transaction raises" do
    with_stub(Sentry, :start_transaction, proc { raise StandardError, "txn-boom" }) do
      assert_nil AuditAppend.start_sentry_transaction
    end
  end

  test "report_sentry_failure swallows errors from Sentry.with_scope" do
    with_stub(Sentry, :with_scope, proc { raise StandardError, "scope-boom" }) do
      assert_nothing_raised do
        AuditAppend.report_sentry_failure(StandardError.new("e"), SecureRandom.uuid, SecureRandom.uuid)
      end
    end
  end

  test "report_sentry_failure sets scope tags and captures message" do
    scope = Object.new
    tags = []
    scope.define_singleton_method(:set_tag) { |k, v| tags << [k, v] }
    msgs = []
    with_stub(Sentry, :with_scope, proc { |&b| b.call(scope) }) do
      with_stub(Sentry, :capture_message, proc { |msg, **| msgs << msg }) do
        lid = SecureRandom.uuid
        AuditAppend.report_sentry_failure(StandardError.new("snap"), "org-z", lid)
        assert_includes tags.flatten, "ledger"
        assert(msgs.any? { |m| m.include?("snap") })
      end
    end
  end

  test "emit_ledger_transaction_posted uses real grpc stub when factory unset" do
    AuditAppend.audit_stub_factory = nil
    ENV["AUDIT_GRPC_URL"] = "127.0.0.1:1"
    ENV["AUDIT_APPEND_TIMEOUT_MS"] = "300"
    tx = sample_tx
    reported = []
    orig = AuditAppend.method(:report_sentry_failure)
    AuditAppend.define_singleton_method(:report_sentry_failure) do |e, oid, lid|
      reported << [e.class.name, oid, lid]
      orig.call(e, oid, lid)
    end
    begin
      AuditAppend.emit_ledger_transaction_posted(tx)
    ensure
      AuditAppend.define_singleton_method(:report_sentry_failure, orig)
    end
    assert reported.any?, "expected unreachable audit host to surface a grpc error"
  end

  private

  def sample_tx
    LedgerTransaction.create!(
      organization_id: SecureRandom.uuid,
      environment: "sandbox",
      external_transaction_id: SecureRandom.uuid,
      status: "posted",
      idempotency_key: SecureRandom.uuid
    )
  end

  def with_sentry_start_transaction_removed
    sc = nil
    orig = nil
    unless defined?(Sentry) && Sentry.respond_to?(:start_transaction)
      yield
      return
    end

    sc = Sentry.singleton_class
    orig = Sentry.method(:start_transaction)
    sc.remove_method(:start_transaction)
    yield
  ensure
    sc.define_method(:start_transaction, orig) if sc && orig
  end
end
