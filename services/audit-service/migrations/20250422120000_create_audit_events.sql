CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    occurred_at TIMESTAMPTZ NOT NULL,
    schema_version SMALLINT NOT NULL,
    source_service TEXT NOT NULL,
    organization_id UUID NOT NULL,
    environment TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT NOT NULL DEFAULT '',
    actor_roles TEXT[] NOT NULL DEFAULT '{}',
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_method TEXT NOT NULL,
    request_path TEXT NOT NULL,
    request_ip TEXT NOT NULL,
    request_user_agent TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    reason TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_events_org_occurred ON audit_events (organization_id, occurred_at);
CREATE INDEX idx_audit_events_correlation ON audit_events (correlation_id);
CREATE INDEX idx_audit_events_action ON audit_events (action);

CREATE RULE prevent_audit_event_updates AS ON UPDATE TO audit_events DO INSTEAD NOTHING;
CREATE RULE prevent_audit_event_deletes AS ON DELETE TO audit_events DO INSTEAD NOTHING;
