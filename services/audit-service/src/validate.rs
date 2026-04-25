//! Protobuf event validation (v1 catalog + PII-safe metadata rules).

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use tonic::Status;
use uuid::Uuid;

use crate::proto::proto::{ActorType, AuditEvent, Outcome};

/// Actions that may use the all-zero organization UUID (no tenant yet).
pub const ORG_ZERO_ALLOWED: &[&str] = &[
    "users.business.register",
    "users.auth.login",
    "users.password_reset.request",
    "users.password_reset.complete",
    "users.beta.apply",
];

/// Full v1 mutation catalog.
pub const ALL_ACTIONS: &[&str] = &[
    "users.business.register",
    "users.auth.login",
    "users.auth.refresh",
    "users.auth.revoke",
    "users.password_reset.request",
    "users.password_reset.complete",
    "users.beta.apply",
    "users.api_key.create",
    "users.api_key.revoke",
    "accounts.account.create",
    "accounts.account.update_status",
    "accounts.account.close",
    "accounts.money.deposit",
    "accounts.money.withdraw",
    "accounts.money.transfer",
    "ledger.transaction.post",
];

pub const METADATA_KEYS_ALLOWED: &[&str] = &[
    "previous_status",
    "new_status",
    "http_status",
    "error_code",
    "idempotency_key_present",
];

const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

fn static_action_set() -> HashSet<&'static str> {
    ALL_ACTIONS.iter().copied().collect()
}

fn static_metadata_keys() -> HashSet<&'static str> {
    METADATA_KEYS_ALLOWED.iter().copied().collect()
}

pub fn validate_audit_event(event: &AuditEvent) -> Result<(), Status> {
    validate_schema(event)?;
    validate_occurred_at(event)?;
    validate_source_service(event)?;
    validate_org_and_action(event)?;
    validate_environment_and_correlation(event)?;
    validate_outcome(event)?;
    validate_actor(event)?;
    validate_target(event)?;
    validate_request(event)?;
    validate_reason(event)?;
    validate_metadata(event)?;
    Ok(())
}

fn validate_schema(event: &AuditEvent) -> Result<(), Status> {
    if event.schema_version != 1 {
        return Err(Status::invalid_argument("schema_version must be 1"));
    }
    Ok(())
}

fn validate_occurred_at(event: &AuditEvent) -> Result<(), Status> {
    let occurred_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&event.occurred_at)
        .map_err(|_| Status::invalid_argument("occurred_at must be RFC3339 UTC"))?
        .with_timezone(&Utc);

    if occurred_at > Utc::now() + chrono::Duration::minutes(5) {
        return Err(Status::invalid_argument("occurred_at cannot be far in the future"));
    }
    Ok(())
}

fn validate_source_service(event: &AuditEvent) -> Result<(), Status> {
    let source = event.source_service.trim();
    if !matches!(source, "users" | "accounts" | "ledger") {
        return Err(Status::invalid_argument(
            "source_service must be one of: users, accounts, ledger",
        ));
    }
    Ok(())
}

fn validate_org_and_action(event: &AuditEvent) -> Result<(), Status> {
    let org_trim = event.organization_id.trim();
    let org = Uuid::parse_str(org_trim)
        .map_err(|_| Status::invalid_argument("organization_id must be a UUID"))?;

    let actions = static_action_set();
    let action = event.action.trim();
    if !actions.contains(action) {
        return Err(Status::invalid_argument("unknown action for v1 catalog"));
    }

    if org.as_hyphenated().to_string() == NIL_UUID {
        if !ORG_ZERO_ALLOWED.contains(&action) {
            return Err(Status::invalid_argument(
                "organization_id must not be all-zero for this action",
            ));
        }
    }
    Ok(())
}

fn validate_environment_and_correlation(event: &AuditEvent) -> Result<(), Status> {
    if event.environment.trim().is_empty() {
        return Err(Status::invalid_argument("environment is required"));
    }

    if event.correlation_id.trim().is_empty() {
        return Err(Status::invalid_argument("correlation_id is required"));
    }
    Ok(())
}

fn validate_outcome(event: &AuditEvent) -> Result<(), Status> {
    let outcome = Outcome::try_from(event.outcome)
        .map_err(|_| Status::invalid_argument("invalid outcome enum value"))?;
    if outcome == Outcome::Unspecified {
        return Err(Status::invalid_argument("outcome must be set"));
    }
    Ok(())
}

fn validate_actor(event: &AuditEvent) -> Result<(), Status> {
    let actor = event.actor.as_ref().ok_or_else(|| Status::invalid_argument("actor is required"))?;
    let actor_type = ActorType::try_from(actor.r#type)
        .map_err(|_| Status::invalid_argument("invalid actor type"))?;
    if actor_type == ActorType::Unspecified {
        return Err(Status::invalid_argument("actor.type must be set"));
    }
    Ok(())
}

fn validate_target(event: &AuditEvent) -> Result<(), Status> {
    let target = event
        .target
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("target is required"))?;
    if target.r#type.trim().is_empty() || target.id.trim().is_empty() {
        return Err(Status::invalid_argument("target.type and target.id are required"));
    }
    Ok(())
}

fn validate_request(event: &AuditEvent) -> Result<(), Status> {
    let req = event
        .request
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("request is required"))?;
    if req.method.trim().is_empty() || req.path.trim().is_empty() {
        return Err(Status::invalid_argument("request.method and request.path are required"));
    }
    Ok(())
}

fn validate_reason(event: &AuditEvent) -> Result<(), Status> {
    if let Some(reason) = event.reason.as_ref() {
        let r = reason.trim();
        if r.chars().count() > 500 {
            return Err(Status::invalid_argument("reason exceeds 500 characters"));
        }
    }
    Ok(())
}

fn validate_metadata(event: &AuditEvent) -> Result<(), Status> {
    let allowed_meta = static_metadata_keys();
    if event.metadata.len() > 16 {
        return Err(Status::invalid_argument("metadata may have at most 16 entries"));
    }
    for (k, v) in &event.metadata {
        if !allowed_meta.contains(k.as_str()) {
            return Err(Status::invalid_argument("metadata key not allowlisted for v1"));
        }
        if v.chars().count() > 256 {
            return Err(Status::invalid_argument("metadata value exceeds 256 characters"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::proto::{Actor, RequestContext, Target};

    fn base_event() -> AuditEvent {
        AuditEvent {
            occurred_at: Utc::now().to_rfc3339(),
            schema_version: 1,
            source_service: "users".into(),
            organization_id: NIL_UUID.into(),
            environment: "unknown".into(),
            actor: Some(Actor {
                r#type: ActorType::Anonymous as i32,
                id: String::default(),
                roles: vec![],
            }),
            action: "users.auth.login".into(),
            target: Some(Target {
                r#type: "user".into(),
                id: NIL_UUID.into(),
            }),
            outcome: Outcome::ClientError as i32,
            request: Some(RequestContext {
                id: Uuid::new_v4().to_string(),
                method: "POST".into(),
                path: "/api/v1/auth/login".into(),
                ip: "127.0.0.1".into(),
                user_agent: "test".into(),
            }),
            correlation_id: "cid".into(),
            reason: None,
            metadata: std::collections::HashMap::default(),
        }
    }

    #[test]
    fn accepts_valid_zero_org_login() {
        validate_audit_event(&base_event()).expect("ok");
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut e = base_event();
        e.schema_version = 2;
        assert!(validate_audit_event(&e).is_err());
    }

    #[test]
    fn rejects_unknown_action() {
        let mut e = base_event();
        e.action = "users.unknown".into();
        assert!(validate_audit_event(&e).is_err());
    }

    #[test]
    fn rejects_zero_org_for_disallowed_action() {
        let mut e = base_event();
        e.action = "users.api_key.create".into();
        assert!(validate_audit_event(&e).is_err());
    }

    #[test]
    fn rejects_bad_metadata_key() {
        let mut e = base_event();
        e.metadata.insert("evil".into(), "x".into());
        assert!(validate_audit_event(&e).is_err());
    }

    #[test]
    fn rejects_metadata_value_too_long() {
        let mut e = base_event();
        e.organization_id = Uuid::new_v4().to_string();
        e.metadata
            .insert("http_status".into(), "x".repeat(257));
        assert!(validate_audit_event(&e).is_err());
    }
}
