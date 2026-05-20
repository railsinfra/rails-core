ALTER TABLE audit_events
    ADD CONSTRAINT audit_events_environment_check
    CHECK (environment IN ('sandbox', 'production')) NOT VALID;
