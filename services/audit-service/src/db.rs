//! Persistence for audit rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditInsert {
    pub occurred_at: DateTime<Utc>,
    pub schema_version: i16,
    pub source_service: String,
    pub organization_id: Uuid,
    pub environment: String,
    pub actor_type: String,
    pub actor_id: String,
    pub actor_roles: Vec<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub outcome: String,
    pub request_id: String,
    pub request_method: String,
    pub request_path: String,
    pub request_ip: String,
    pub request_user_agent: String,
    pub correlation_id: String,
    pub reason: Option<String>,
    pub metadata: Value,
}

pub async fn insert_audit_event(pool: &PgPool, row: AuditInsert) -> Result<Uuid, sqlx::Error> {
    let rec = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO audit_events (
            occurred_at, schema_version, source_service, organization_id, environment,
            actor_type, actor_id, actor_roles, action, target_type, target_id, outcome,
            request_id, request_method, request_path, request_ip, request_user_agent,
            correlation_id, reason, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
        RETURNING id
        "#,
    )
    .bind(row.occurred_at)
    .bind(row.schema_version)
    .bind(&row.source_service)
    .bind(row.organization_id)
    .bind(&row.environment)
    .bind(&row.actor_type)
    .bind(&row.actor_id)
    .bind(&row.actor_roles)
    .bind(&row.action)
    .bind(&row.target_type)
    .bind(&row.target_id)
    .bind(&row.outcome)
    .bind(&row.request_id)
    .bind(&row.request_method)
    .bind(&row.request_path)
    .bind(&row.request_ip)
    .bind(&row.request_user_agent)
    .bind(&row.correlation_id)
    .bind(row.reason.as_deref())
    .bind(row.metadata)
    .fetch_one(pool)
    .await?;
    Ok(rec)
}
