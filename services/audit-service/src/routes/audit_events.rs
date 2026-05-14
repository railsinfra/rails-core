use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::Code;
use uuid::Uuid;

use crate::db::{
    insert_audit_event, list_audit_events as list_audit_events_db, AuditEventRow, AuditListFilters,
    AuditPagination,
};
use crate::grpc_server::event_to_insert;
use crate::proto::proto::{Actor, ActorType, AuditEvent, Outcome, RequestContext, Target};
use crate::routes::AppState;
use crate::validate::validate_audit_event;

type ApiError = (StatusCode, Json<ErrorResponse>);
type ApiResult<T> = Result<(StatusCode, Json<T>), ApiError>;

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse {
    status: u16,
    message: String,
    #[serde(rename = "correlationId")]
    correlation_id: String,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
pub struct AppendAuditEventRequest {
    event: AuditEventPayload,
}

#[derive(Debug, Deserialize)]
struct AuditEventPayload {
    occurred_at: String,
    schema_version: i32,
    source_service: String,
    organization_id: String,
    environment: String,
    actor: ActorPayload,
    action: String,
    target: TargetPayload,
    outcome: String,
    request: RequestContextPayload,
    correlation_id: String,
    reason: Option<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ActorPayload {
    #[serde(rename = "type")]
    actor_type: String,
    id: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TargetPayload {
    #[serde(rename = "type")]
    target_type: String,
    id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RequestContextPayload {
    id: String,
    method: String,
    path: String,
    #[serde(default)]
    ip: String,
    #[serde(default)]
    user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct AppendAuditEventResponse {
    audit_event_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListAuditEventsQuery {
    organization_id: Option<String>,
    environment: Option<String>,
    action: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
    outcome: Option<String>,
    from: Option<String>,
    to: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListAuditEventsResponse {
    data: Vec<AuditEventResponse>,
    pagination: PaginationResponse,
}

#[derive(Debug, Serialize)]
pub struct PaginationResponse {
    page: u32,
    per_page: u32,
    total_count: i64,
    total_pages: u32,
}

#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    id: String,
    occurred_at: String,
    schema_version: i16,
    source_service: String,
    organization_id: String,
    environment: String,
    actor: ActorResponse,
    action: String,
    target: TargetPayload,
    outcome: String,
    request: RequestContextPayload,
    correlation_id: String,
    reason: Option<String>,
    metadata: serde_json::Value,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ActorResponse {
    #[serde(rename = "type")]
    actor_type: String,
    id: String,
    roles: Vec<String>,
}

pub async fn append_audit_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AppendAuditEventRequest>,
) -> ApiResult<AppendAuditEventResponse> {
    let internal_token = header_value(&headers, "x-internal-service-token").ok_or_else(|| {
        api_error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "X-Internal-Service-Token header is required",
        )
    })?;
    let expected = state.internal_token.as_deref().ok_or_else(|| {
        api_error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Internal audit append is not configured",
        )
    })?;
    if internal_token != expected {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Invalid internal service token",
        ));
    }

    if let Some(environment) = header_value(&headers, "x-environment")
        .map(|value| normalize_environment(Some(&value)))
        .transpose()?
    {
        if request.event.environment.trim().to_lowercase() != environment {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "VALIDATION_FAILED",
                "event.environment must match X-Environment",
            ));
        }
    }

    if Uuid::parse_str(request.event.organization_id.trim()).is_err() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "organization_id must be a UUID",
        ));
    }

    let event = request.event.into_proto()?;
    validate_audit_event(&event).map_err(map_tonic_validation_error)?;
    let row = event_to_insert(&event).map_err(map_tonic_validation_error)?;
    let id = insert_audit_event(&state.pool, row).await.map_err(|e| {
        tracing::error!(error = %e, "failed to insert SDK audit event");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Failed to append audit event",
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(AppendAuditEventResponse {
            audit_event_id: id.to_string(),
        }),
    ))
}

pub async fn list_audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListAuditEventsQuery>,
) -> ApiResult<ListAuditEventsResponse> {
    let (organization_id, environment) = resolve_list_scope(&state, &headers, &query).await?;
    let filters = build_list_filters(organization_id, environment, query)?;

    let (rows, pagination) = list_audit_events_db(&state.pool, &filters)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list audit events");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                "Failed to list audit events",
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(ListAuditEventsResponse {
            data: rows.into_iter().map(AuditEventResponse::from).collect(),
            pagination: PaginationResponse::from(pagination),
        }),
    ))
}

async fn resolve_list_scope(
    state: &AppState,
    headers: &HeaderMap,
    query: &ListAuditEventsQuery,
) -> Result<(Uuid, String), ApiError> {
    if let Some(internal_token) = header_value(headers, "x-internal-service-token") {
        let expected = state.internal_token.as_deref().ok_or_else(|| {
            api_error(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Internal audit reads are not configured",
            )
        })?;
        if internal_token != expected {
            return Err(api_error(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Invalid internal service token",
            ));
        }

        let organization_id = query
            .organization_id
            .as_deref()
            .ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    "VALIDATION_FAILED",
                    "organization_id query parameter is required",
                )
            })
            .and_then(parse_uuid)?;
        let environment = normalize_environment(query.environment.as_deref())?;
        return Ok((organization_id, environment));
    }

    let environment = query
        .environment
        .as_deref()
        .map(|s| normalize_environment(Some(s)))
        .transpose()?
        .or_else(|| header_value(headers, "x-environment"))
        .map(|s| normalize_environment(Some(&s)))
        .transpose()?
        .unwrap_or_else(|| "sandbox".to_string());

    let api_key = header_value(headers, "x-api-key").ok_or_else(|| {
        api_error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "X-API-Key header is required",
        )
    })?;

    let (business_id, _environment_id, _admin_user_id) = state
        .users_grpc
        .validate_api_key(&api_key, &environment)
        .await
        .map_err(map_users_grpc_error)?;

    Ok((business_id, environment))
}

fn build_list_filters(
    organization_id: Uuid,
    environment: String,
    query: ListAuditEventsQuery,
) -> Result<AuditListFilters, ApiError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 100);
    let outcome = optional_trimmed(query.outcome)
        .map(validate_outcome_filter)
        .transpose()?;

    Ok(AuditListFilters {
        organization_id,
        environment,
        action: optional_trimmed(query.action),
        target_type: optional_trimmed(query.target_type),
        target_id: optional_trimmed(query.target_id),
        outcome,
        from: query.from.as_deref().map(parse_rfc3339).transpose()?,
        to: query.to.as_deref().map(parse_rfc3339).transpose()?,
        page,
        per_page,
    })
}

impl AuditEventPayload {
    fn into_proto(self) -> Result<AuditEvent, ApiError> {
        Ok(AuditEvent {
            occurred_at: self.occurred_at,
            schema_version: self.schema_version,
            source_service: self.source_service,
            organization_id: self.organization_id,
            environment: self.environment,
            actor: Some(Actor {
                r#type: parse_actor_type(&self.actor.actor_type)? as i32,
                id: self.actor.id,
                roles: self.actor.roles,
            }),
            action: self.action,
            target: Some(Target {
                r#type: self.target.target_type,
                id: self.target.id,
            }),
            outcome: parse_outcome(&self.outcome)? as i32,
            request: Some(RequestContext {
                id: self.request.id,
                method: self.request.method,
                path: self.request.path,
                ip: self.request.ip,
                user_agent: self.request.user_agent,
            }),
            correlation_id: self.correlation_id,
            reason: self.reason,
            metadata: self.metadata,
        })
    }
}

impl From<AuditEventRow> for AuditEventResponse {
    fn from(row: AuditEventRow) -> Self {
        Self {
            id: row.id.to_string(),
            occurred_at: row.occurred_at.to_rfc3339(),
            schema_version: row.schema_version,
            source_service: row.source_service,
            organization_id: row.organization_id.to_string(),
            environment: row.environment,
            actor: ActorResponse {
                actor_type: row.actor_type,
                id: row.actor_id,
                roles: row.actor_roles,
            },
            action: row.action,
            target: TargetPayload {
                target_type: row.target_type,
                id: row.target_id,
            },
            outcome: row.outcome,
            request: RequestContextPayload {
                id: row.request_id,
                method: row.request_method,
                path: row.request_path,
                ip: row.request_ip,
                user_agent: row.request_user_agent,
            },
            correlation_id: row.correlation_id,
            reason: row.reason,
            metadata: row.metadata,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

impl From<AuditPagination> for PaginationResponse {
    fn from(pagination: AuditPagination) -> Self {
        Self {
            page: pagination.page,
            per_page: pagination.per_page,
            total_count: pagination.total_count,
            total_pages: pagination.total_pages,
        }
    }
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_uuid(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value.trim()).map_err(|_| {
        api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "organization_id must be a UUID",
        )
    })
}

fn normalize_environment(value: Option<&str>) -> Result<String, ApiError> {
    match value.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("sandbox") => Ok("sandbox".to_string()),
        Some("production") => Ok("production".to_string()),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "environment must be 'sandbox' or 'production'",
        )),
    }
}

fn validate_outcome_filter(value: String) -> Result<String, ApiError> {
    match value.as_str() {
        "success" | "client_error" | "server_error" => Ok(value),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "outcome must be one of: success, client_error, server_error",
        )),
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                "VALIDATION_FAILED",
                "from and to must be RFC3339 timestamps",
            )
        })
}

fn parse_actor_type(value: &str) -> Result<ActorType, (StatusCode, Json<ErrorResponse>)> {
    match value.trim() {
        "user" => Ok(ActorType::User),
        "api_key" => Ok(ActorType::ApiKey),
        "internal_service" => Ok(ActorType::InternalService),
        "anonymous" => Ok(ActorType::Anonymous),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "actor.type must be one of: user, api_key, internal_service, anonymous",
        )),
    }
}

fn parse_outcome(value: &str) -> Result<Outcome, (StatusCode, Json<ErrorResponse>)> {
    match value.trim() {
        "success" => Ok(Outcome::Success),
        "client_error" => Ok(Outcome::ClientError),
        "server_error" => Ok(Outcome::ServerError),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "outcome must be one of: success, client_error, server_error",
        )),
    }
}

fn map_users_grpc_error(status: tonic::Status) -> (StatusCode, Json<ErrorResponse>) {
    match status.code() {
        Code::Unauthenticated | Code::InvalidArgument => {
            api_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", status.message())
        }
        Code::PermissionDenied => api_error(StatusCode::FORBIDDEN, "FORBIDDEN", status.message()),
        _ => {
            tracing::error!(error = %status, "users-service API key validation failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                "Failed to validate API key",
            )
        }
    }
}

fn map_tonic_validation_error(status: tonic::Status) -> (StatusCode, Json<ErrorResponse>) {
    api_error(
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        status.message(),
    )
}

fn api_error(
    status: StatusCode,
    _code: &'static str,
    message: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            status: status.as_u16(),
            message: message.into(),
            correlation_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }),
    )
}
