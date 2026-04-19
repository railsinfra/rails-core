use axum::{Json, extract::State, extract::Path};
use uuid::Uuid;
use crate::{error::AppError};
use crate::routes::AppState;
use crate::auth::{AuthContext, hash_api_key};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use sqlx::Row;
use rand::rngs::OsRng;
use rand::rand_core::TryRngCore;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_ENGINE;
use base64::Engine;

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub environment_id: Option<Uuid>
}

#[derive(Serialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub key: String,
    pub status: String
}

#[derive(Serialize)]
pub struct ApiKeyInfo {
    pub id: Uuid,
    pub business_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub status: String,
    pub last_used_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
    pub revoked_at: Option<chrono::DateTime<Utc>>,
    pub created_by_user_id: Option<Uuid>,
}

pub async fn create_api_key(
    State(state): State<AppState>,
    ctx: AuthContext,
    Json(payload): Json<CreateApiKeyRequest>
) -> Result<Json<CreateApiKeyResponse>, AppError> {
    let request_user_id = ctx.user_id.ok_or(AppError::Forbidden)?;

    let role_row = sqlx::query("SELECT role FROM users WHERE id = $1 AND environment_id = $2 AND status = 'active'")
        .bind(&request_user_id)
        .bind(&ctx.environment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Forbidden)?;

    let role: String = role_row.get("role");
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    if let Some(env_id) = payload.environment_id {
        let env_ok = sqlx::query(
            "SELECT 1 FROM environments WHERE id = $1 AND business_id = $2 AND status = 'active'"
        )
        .bind(&env_id)
        .bind(&ctx.business_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
        if env_ok.is_none() {
            return Err(AppError::BadRequest("Invalid environment_id".to_string()));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let mut raw = [0u8; 32];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut raw)
        .map_err(|_| AppError::Internal)?;
    let api_key_plain = BASE64_URL_ENGINE.encode(raw);
    let key_hash = hash_api_key(&api_key_plain)?;

    sqlx::query(
        "INSERT INTO api_keys (id, business_id, environment_id, key_hash, status, last_used_at, created_at, revoked_at, created_by_user_id) VALUES ($1, $2, $3, $4, 'active', NULL, $5, NULL, $6)"
    )
    .bind(&id)
    .bind(&ctx.business_id)
    .bind(&payload.environment_id)
    .bind(&key_hash)
    .bind(&now)
    .bind(&request_user_id)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(CreateApiKeyResponse {
        id,
        key: api_key_plain,
        status: "active".to_string(),
    }))
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    ctx: AuthContext,
) -> Result<Json<Vec<ApiKeyInfo>>, AppError> {
    let request_user_id = ctx.user_id.ok_or(AppError::Forbidden)?;
    let role_row = sqlx::query("SELECT role FROM users WHERE id = $1 AND environment_id = $2 AND status = 'active'")
        .bind(&request_user_id)
        .bind(&ctx.environment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Forbidden)?;

    let role: String = role_row.get("role");
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    let rows = sqlx::query(
        "SELECT id, business_id, environment_id, status, last_used_at, created_at, revoked_at, created_by_user_id FROM api_keys WHERE business_id = $1 ORDER BY created_at DESC"
    )
    .bind(&ctx.business_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let keys = rows
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.get("id"),
            business_id: row.get("business_id"),
            environment_id: row.get("environment_id"),
            status: row.get("status"),
            last_used_at: row.get("last_used_at"),
            created_at: row.get("created_at"),
            revoked_at: row.get("revoked_at"),
            created_by_user_id: row.get("created_by_user_id"),
        })
        .collect();

    Ok(Json(keys))
}

pub async fn revoke_api_key(
    State(state): State<AppState>,
    ctx: AuthContext,
    Path(api_key_id): Path<Uuid>
) -> Result<Json<CreateApiKeyResponse>, AppError> {
    let request_user_id = ctx.user_id.ok_or(AppError::Forbidden)?;
    let role_row = sqlx::query("SELECT role FROM users WHERE id = $1 AND environment_id = $2 AND status = 'active'")
        .bind(&request_user_id)
        .bind(&ctx.environment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Forbidden)?;

    let role: String = role_row.get("role");
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    let now = Utc::now();
    let result = sqlx::query(
        "UPDATE api_keys SET status = 'revoked', revoked_at = $1 WHERE id = $2 AND business_id = $3 AND status = 'active'"
    )
    .bind(&now)
    .bind(&api_key_id)
    .bind(&ctx.business_id)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("API key not found or already revoked".to_string()));
    }

    Ok(Json(CreateApiKeyResponse {
        id: api_key_id,
        key: "".to_string(),
        status: "revoked".to_string(),
    }))
}
