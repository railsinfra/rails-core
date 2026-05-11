//! Persistence for audit rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
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

#[derive(Debug, Clone)]
pub struct AuditEventRow {
    pub id: Uuid,
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuditListFilters {
    pub organization_id: Uuid,
    pub environment: String,
    pub action: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub outcome: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Clone)]
pub struct AuditPagination {
    pub page: u32,
    pub per_page: u32,
    pub total_count: i64,
    pub total_pages: u32,
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

pub async fn list_audit_events(
    pool: &PgPool,
    filters: &AuditListFilters,
) -> Result<(Vec<AuditEventRow>, AuditPagination), sqlx::Error> {
    let total_count = count_audit_events(pool, filters).await?;
    let total_pages = ((total_count as f64) / (filters.per_page as f64)).ceil() as u32;
    let offset = (filters.page - 1) * filters.per_page;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        r#"
        SELECT id, occurred_at, schema_version, source_service, organization_id, environment,
               actor_type, actor_id, actor_roles, action, target_type, target_id, outcome,
               request_id, request_method, request_path, request_ip, request_user_agent,
               correlation_id, reason, metadata, created_at
        FROM audit_events
        "#,
    );
    push_filter_clause(&mut qb, filters);
    qb.push(" ORDER BY occurred_at DESC, created_at DESC, id DESC LIMIT ");
    qb.push_bind(filters.per_page as i64);
    qb.push(" OFFSET ");
    qb.push_bind(offset as i64);

    let rows = qb.build().fetch_all(pool).await?;
    let events = rows
        .iter()
        .map(row_to_audit_event)
        .collect::<Result<Vec<_>, _>>()?;

    Ok((
        events,
        AuditPagination {
            page: filters.page,
            per_page: filters.per_page,
            total_count,
            total_pages,
        },
    ))
}

async fn count_audit_events(pool: &PgPool, filters: &AuditListFilters) -> Result<i64, sqlx::Error> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT COUNT(*) FROM audit_events");
    push_filter_clause(&mut qb, filters);
    qb.build_query_scalar::<i64>().fetch_one(pool).await
}

fn push_filter_clause(qb: &mut QueryBuilder<Postgres>, filters: &AuditListFilters) {
    qb.push(" WHERE organization_id = ");
    qb.push_bind(filters.organization_id);
    qb.push(" AND environment = ");
    qb.push_bind(filters.environment.clone());

    if let Some(action) = &filters.action {
        qb.push(" AND action = ");
        qb.push_bind(action.clone());
    }
    if let Some(target_type) = &filters.target_type {
        qb.push(" AND target_type = ");
        qb.push_bind(target_type.clone());
    }
    if let Some(target_id) = &filters.target_id {
        qb.push(" AND target_id = ");
        qb.push_bind(target_id.clone());
    }
    if let Some(outcome) = &filters.outcome {
        qb.push(" AND outcome = ");
        qb.push_bind(outcome.clone());
    }
    if let Some(from) = filters.from {
        qb.push(" AND occurred_at >= ");
        qb.push_bind(from);
    }
    if let Some(to) = filters.to {
        qb.push(" AND occurred_at <= ");
        qb.push_bind(to);
    }
}

fn row_to_audit_event(row: &sqlx::postgres::PgRow) -> Result<AuditEventRow, sqlx::Error> {
    Ok(AuditEventRow {
        id: row.try_get("id")?,
        occurred_at: row.try_get("occurred_at")?,
        schema_version: row.try_get("schema_version")?,
        source_service: row.try_get("source_service")?,
        organization_id: row.try_get("organization_id")?,
        environment: row.try_get("environment")?,
        actor_type: row.try_get("actor_type")?,
        actor_id: row.try_get("actor_id")?,
        actor_roles: row.try_get("actor_roles")?,
        action: row.try_get("action")?,
        target_type: row.try_get("target_type")?,
        target_id: row.try_get("target_id")?,
        outcome: row.try_get("outcome")?,
        request_id: row.try_get("request_id")?,
        request_method: row.try_get("request_method")?,
        request_path: row.try_get("request_path")?,
        request_ip: row.try_get("request_ip")?,
        request_user_agent: row.try_get("request_user_agent")?,
        correlation_id: row.try_get("correlation_id")?,
        reason: row.try_get("reason")?,
        metadata: row.try_get("metadata")?,
        created_at: row.try_get("created_at")?,
    })
}
