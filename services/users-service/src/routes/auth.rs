use axum::{Json, extract::State};
use argon2::password_hash::rand_core::OsRng;
use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::engine::Engine;
use crate::error::AppError;
use crate::routes::{AppState, user};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub environment_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub selected_environment_id: Uuid,
    pub environments: Vec<EnvironmentInfo>,
}

#[derive(Serialize)]
pub struct EnvironmentInfo {
    pub id: Uuid,
    pub r#type: String,
}

#[derive(Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Deserialize)]
pub struct RevokeTokenRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct RevokeTokenResponse {
    pub status: String,
}

use jsonwebtoken::{encode, EncodingKey, Header};
use argon2::password_hash::{PasswordHash, PasswordVerifier, rand_core::RngCore};
use chrono::{Utc, Duration};
use uuid::Uuid;
use serde_json::json;

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>
) -> Result<Json<LoginResponse>, AppError> {
    let email_normalized = user::normalize_email(&payload.email);
    // Optimized: Get active users with their environment_ids and business_id in one query
    let user_rows = sqlx::query(
        "SELECT id, password_hash, status, environment_id, business_id FROM users WHERE email = $1 AND status = 'active'"
    )
    .bind(&email_normalized)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error in login (user lookup): {}", e);
        AppError::Internal
    })?;

    if user_rows.is_empty() {
        return Err(AppError::Unauthorized);
    }

    // Get password hash from first active user (should only be one per email in practice)
    let password_hash: String = user_rows[0].get("password_hash");

    // 2. Verify password
    let parsed_hash = PasswordHash::new(&password_hash).map_err(|e| {
        let user_id: Uuid = user_rows[0].get("id");
        tracing::error!("Failed to parse password hash for user_id {}: {}", user_id, e);
        AppError::Internal
    })?;
    if argon2::Argon2::default().verify_password(payload.password.as_bytes(), &parsed_hash).is_err() {
        return Err(AppError::Unauthorized);
    }

    // Get business_id from first user (all users with same email should have same business_id)
    let business_id: Uuid = user_rows[0].get("business_id");
    
    // Get ALL environments for the business (not just where user exists)
    // This allows users to see both sandbox and production environments
    let environments = sqlx::query(
        "SELECT id, type FROM environments WHERE business_id = $1 AND status = 'active' ORDER BY type"
    )
    .bind(&business_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error in login (environments lookup): {}", e);
        AppError::Internal
    })?;

    if environments.is_empty() {
        return Err(AppError::Unauthorized);
    }

    let mut available_envs: Vec<EnvironmentInfo> = environments
        .iter()
        .map(|row| EnvironmentInfo {
            id: row.get("id"),
            r#type: row.get("type"),
        })
        .collect();

    // Sort: sandbox first, then production
    available_envs.sort_by(|a, b| a.r#type.cmp(&b.r#type));

    let requested_env_id = payload.environment_id;
    let sandbox_env_id = available_envs
        .iter()
        .find(|e| e.r#type == "sandbox")
        .map(|e| e.id);

    let selected_environment_id = if let Some(env_id) = requested_env_id {
        if available_envs.iter().any(|e| e.id == env_id) {
            env_id
        } else if let Some(sandbox_id) = sandbox_env_id {
            sandbox_id
        } else {
            available_envs[0].id
        }
    } else if let Some(sandbox_id) = sandbox_env_id {
        sandbox_id
    } else {
        available_envs[0].id
    };

    // Find user in the selected environment
    let user_id = user_rows
        .iter()
        .find(|row| {
            let env_id: Uuid = row.get("environment_id");
            let status: String = row.get("status");
            status == "active" && env_id == selected_environment_id
        })
        .map(|row| row.get::<Uuid, _>("id"))
        .ok_or_else(|| AppError::Unauthorized)?;

    // 3. Generate JWT access token
    let jwt_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let exp = now + Duration::minutes(15);
    let claims = json!({
        "sub": user_id.to_string(),
        "jti": jwt_id,
        "exp": exp.timestamp(),
        "iat": now.timestamp(),
        "env": selected_environment_id.to_string(),
        "business_id": business_id.to_string(),
    });
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev_secret".to_string());
    let access_token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|_| AppError::Internal)?;

    // 4. Generate refresh token
    let mut refresh_bytes = [0u8; 32];
    let mut rng = OsRng;
    RngCore::fill_bytes(&mut rng, &mut refresh_bytes);
    let refresh_token = BASE64_ENGINE.encode(refresh_bytes);
    let refresh_id = Uuid::new_v4();
    let refresh_exp = now + Duration::days(30);

    // 5. Store session
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, environment_id, refresh_token, jwt_id, status, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, 'active', $6, $7)"
    )
    .bind(&refresh_id)
    .bind(&user_id)
    .bind(&selected_environment_id)
    .bind(&refresh_token)
    .bind(&jwt_id)
    .bind(&now)
    .bind(&refresh_exp)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error in login (session insert): {}", e);
        AppError::Internal
    })?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        expires_in: 900,
        selected_environment_id,
        environments: available_envs,
    }))
}

pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>
) -> Result<Json<RefreshTokenResponse>, AppError> {
    // 1. Find session by refresh token
    let rec = sqlx::query(
        "SELECT id, user_id, environment_id, jwt_id, status, expires_at FROM user_sessions WHERE refresh_token = $1"
    )
    .bind(&payload.refresh_token)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    let rec = rec.ok_or_else(|| AppError::Unauthorized)?;
    let session_id: Uuid = rec.get("id");
    let user_id: Uuid = rec.get("user_id");
    let environment_id: Uuid = rec.get("environment_id");
    let status: String = rec.get("status");
    let expires_at: chrono::DateTime<Utc> = rec.get("expires_at");
    
    if status != "active" || expires_at < Utc::now() {
        return Err(AppError::Unauthorized);
    }

    // Get business_id for JWT
    let business_row = sqlx::query(
        "SELECT business_id FROM users WHERE id = $1"
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::Internal)?;
    let business_id: Uuid = business_row.get("business_id");

    // 2. Issue new JWT
    let jwt_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let exp = now + Duration::minutes(15);
    let claims = json!({
        "sub": user_id.to_string(),
        "jti": jwt_id,
        "exp": exp.timestamp(),
        "iat": now.timestamp(),
        "env": environment_id.to_string(),
        "business_id": business_id.to_string(),
    });
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev_secret".to_string());
    let access_token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|_| AppError::Internal)?;

    // 3. Issue new refresh token and update session
    let mut refresh_bytes = [0u8; 32];
    let mut rng = OsRng;
    RngCore::fill_bytes(&mut rng, &mut refresh_bytes);
    let new_refresh_token = BASE64_ENGINE.encode(refresh_bytes);
    let new_refresh_id = Uuid::new_v4();
    let refresh_exp = now + Duration::days(30);

    // Revoke old session and insert new
    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;
    sqlx::query(
        "UPDATE user_sessions SET status = 'revoked', revoked_at = $1 WHERE id = $2"
    )
    .bind(&now)
    .bind(&session_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, environment_id, refresh_token, jwt_id, status, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, 'active', $6, $7)"
    )
    .bind(&new_refresh_id)
    .bind(&user_id)
    .bind(&environment_id)
    .bind(&new_refresh_token)
    .bind(&jwt_id)
    .bind(&now)
    .bind(&refresh_exp)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;
    tx.commit().await.map_err(|_| AppError::Internal)?;

    Ok(Json(RefreshTokenResponse {
        access_token,
        refresh_token: new_refresh_token,
        expires_in: 900,
    }))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Json(payload): Json<RevokeTokenRequest>
) -> Result<Json<RevokeTokenResponse>, AppError> {
    let now = Utc::now();
    let result = sqlx::query(
        "UPDATE user_sessions SET status = 'revoked', revoked_at = $1 WHERE refresh_token = $2 AND status = 'active'"
    )
    .bind(&now)
    .bind(&payload.refresh_token)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Token not found or already revoked".to_string()));
    }
    Ok(Json(RevokeTokenResponse {
        status: "revoked".to_string(),
    }))
}
