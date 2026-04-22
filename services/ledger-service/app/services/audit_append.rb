# frozen_string_literal: true

# Fire-and-forget append to audit-service after a ledger row is durably posted (RAI-14).
# Uses AUDIT_GRPC_URL (host:port). Failures are logged and reported to Sentry; they never roll back the ledger.
class AuditAppend
  TXN_NAME = 'ledger.audit.emit'
  TXN_OP = 'audit.emit'

  def self.append_timeout_sec
    ms = ENV.fetch('AUDIT_APPEND_TIMEOUT_MS', '5000').to_s.to_i
    ms = 5000 if ms <= 0
    ms = 120_000 if ms > 120_000
    ms / 1000.0
  end

  def self.emit_ledger_transaction_posted(ledger_transaction)
    org_id = ledger_transaction.organization_id.to_s
    target = ENV.fetch('AUDIT_GRPC_URL', '').strip
    return if target.empty?

    require Rails.root.join('lib/grpc/audit/v1/audit_services_pb').to_s

    correlation = ledger_transaction.external_transaction_id.to_s

    event = Rails::Core::Audit::V1::AuditEvent.new(
      occurred_at: Time.now.utc.iso8601(3),
      schema_version: 1,
      source_service: 'ledger',
      organization_id: org_id,
      environment: ledger_transaction.environment,
      actor: Rails::Core::Audit::V1::Actor.new(
        type: :ACTOR_TYPE_INTERNAL_SERVICE,
        id: 'ledger',
        roles: []
      ),
      action: 'ledger.transaction.post',
      target: Rails::Core::Audit::V1::Target.new(
        type: 'ledger_transaction',
        id: ledger_transaction.id.to_s
      ),
      outcome: :OUTCOME_SUCCESS,
      request: Rails::Core::Audit::V1::RequestContext.new(
        id: SecureRandom.uuid,
        method: 'POST',
        path: '/grpc/LedgerService/PostTransaction',
        ip: '127.0.0.1',
        user_agent: 'ledger-grpc'
      ),
      correlation_id: correlation,
      metadata: { 'idempotency_key_present' => 'true' }
    )

    req = Rails::Core::Audit::V1::AppendAuditEventRequest.new(event: event)
    timeout_sec = append_timeout_sec
    stub = Rails::Core::Audit::V1::AuditService::Stub.new(
      target,
      :this_channel_is_insecure,
      timeout: timeout_sec
    )

    finish_txn = start_sentry_transaction
    deadline = Time.now + timeout_sec
    stub.append_audit_event(req, deadline: deadline)
  rescue StandardError => e
    Rails.logger.error(
      "[AUDIT] append failed ledger_id=#{ledger_transaction.id} org=#{org_id} grpc=#{e.class}: #{e.message}"
    )
    report_sentry_failure(e, org_id, ledger_transaction.id)
  ensure
    finish_txn&.call
  end

  def self.start_sentry_transaction
    return nil unless defined?(Sentry) && Sentry.respond_to?(:start_transaction)

    txn = Sentry.start_transaction(name: TXN_NAME, op: TXN_OP)
    Sentry.get_current_scope&.set_tags('audit.action' => 'ledger.transaction.post') if txn && Sentry.respond_to?(:get_current_scope)
    proc { txn&.finish }
  rescue StandardError
    nil
  end

  def self.report_sentry_failure(err, org_id, ledger_id)
    return unless defined?(Sentry) && Sentry.respond_to?(:capture_message)

    Sentry.with_scope do |scope|
      if scope
        scope.set_tag('source_service', 'ledger')
        scope.set_tag('audit.action', 'ledger.transaction.post')
        scope.set_tag('organization_id', org_id)
        scope.set_tag('ledger_transaction_id', ledger_id.to_s)
      end
      Sentry.capture_message("audit-append-failure: #{err.message}", level: :error)
    end
  rescue StandardError => sentry_err
    Rails.logger.warn "[AUDIT] Sentry report failed: #{sentry_err.message}"
  end
end
