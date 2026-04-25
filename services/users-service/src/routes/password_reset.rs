//! Password reset endpoints
//! Secure, single-use, time-limited password reset flow

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use axum::{Json, extract::State};

use crate::audit_emit;
use crate::grpc::audit_proto::ActorType;
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{rand_core::{OsRng, RngCore}, SaltString};
use chrono::{Utc, Duration};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sha2::{Sha256, Digest};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_ENGINE;
use base64::Engine;

use crate::error::AppError;
use crate::routes::{AppState, user};

#[derive(Deserialize)]
pub struct RequestPasswordResetRequest {
    pub email: String,
}

#[derive(Serialize)]
pub struct RequestPasswordResetResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct ResetPasswordResponse {
    pub message: String,
}

enum PasswordResetRequestOutcome {
    NoSuchUser,
    Issued { user_id: Uuid, business_id: Uuid },
}

async fn request_password_reset_inner(
    state: AppState,
    Json(payload): Json<RequestPasswordResetRequest>,
) -> Result<(RequestPasswordResetResponse, PasswordResetRequestOutcome), AppError> {
    // Always return success to prevent user enumeration
    // This is a security best practice
    
    // Find user by email (only active users); normalize for case-insensitive lookup
    let email_normalized = user::normalize_email(&payload.email);
    let user_row = sqlx::query(
        "SELECT id FROM users WHERE email = $1 AND status = 'active' LIMIT 1"
    )
    .bind(&email_normalized)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    // If user doesn't exist, still return success (no enumeration)
    let user_id: Uuid = match user_row {
        Some(row) => row.get("id"),
        None => {
            tracing::info!("Password reset requested for non-existent account");
            return Ok((
                RequestPasswordResetResponse {
                    message: "If an account exists with that email, a password reset link has been sent."
                        .to_string(),
                },
                PasswordResetRequestOutcome::NoSuchUser,
            ));
        }
    };

    let raw_token = generate_reset_token();

    // Hash token before storing (never store raw tokens)
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());

    // Set expiry to 1 hour from now
    let expires_at = Utc::now() + Duration::hours(1);
    let token_id = Uuid::new_v4();

    // Use transaction to ensure atomicity
    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;
    let now = Utc::now();

    // Invalidate all previous reset tokens for this user
    // This ensures only the latest token is usable
    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = $1 WHERE user_id = $2 AND used_at IS NULL"
    )
    .bind(&now)
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    // Store new token (hashed)
    sqlx::query(
        "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&token_id)
    .bind(&user_id)
    .bind(&token_hash)
    .bind(&expires_at)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    tx.commit().await.map_err(|_| AppError::Internal)?;

    // Send email (best-effort, don't fail if email fails)
    if let Some(email_service) = &state.email {
        match email_service.send_password_reset(&payload.email, &raw_token).await {
            Ok(_) => {
                tracing::info!("Password reset email sent successfully to {}", payload.email);
            }
            Err(e) => {
                // Log error but don't fail the request
                // User enumeration protection: still return success
                tracing::error!("Failed to send password reset email: {}", e);
            }
        }
    } else {
        tracing::warn!("Email service not configured, password reset email not sent");
    }

    let biz_row = sqlx::query("SELECT business_id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let business_id: Uuid = biz_row.get("business_id");

    Ok((
        RequestPasswordResetResponse {
            message: "If an account exists with that email, a password reset link has been sent.".to_string(),
        },
        PasswordResetRequestOutcome::Issued {
            user_id,
            business_id,
        },
    ))
}

/// Request password reset
/// Always returns success to prevent user enumeration
/// If user exists, generates token and sends email
pub async fn request_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<RequestPasswordResetRequest>,
) -> Result<Json<RequestPasswordResetResponse>, AppError> {
    let path = "/api/v1/auth/password-reset/request";
    let mut meta = HashMap::new();
    match request_password_reset_inner(state.clone(), Json(payload)).await {
        Ok((body, outcome)) => {
            let (org, actor, actor_id, tgt) = match &outcome {
                PasswordResetRequestOutcome::NoSuchUser => {
                    (Uuid::nil(), ActorType::Anonymous, String::default(), Uuid::nil())
                }
                PasswordResetRequestOutcome::Issued {
                    user_id,
                    business_id,
                } => (
                    *business_id,
                    ActorType::User,
                    user_id.to_string(),
                    *user_id,
                ),
            };
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.password_reset.request",
                org,
                actor,
                &actor_id,
                vec![],
                "user",
                tgt,
                200,
                None,
                meta,
            )
            .await;
            Ok(Json(body))
        }
        Err(e) => {
            meta.insert(
                "http_status".into(),
                audit_emit::http_status_for_error(&e).to_string(),
            );
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.password_reset.request",
                Uuid::nil(),
                ActorType::Anonymous,
                "",
                vec![],
                "user",
                Uuid::nil(),
                audit_emit::http_status_for_error(&e),
                Some(audit_emit::truncate_reason(&e.to_string())),
                meta,
            )
            .await;
            Err(e)
        }
    }
}

fn generate_reset_token() -> String {
    // Generate URL-safe token to avoid '+' '/' '=' in query params.
    let mut token_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut token_bytes);
    BASE64_ENGINE.encode(token_bytes)
}

#[cfg(test)]
mod tests {
    use super::{claim_token_sql, generate_reset_token, revoke_user_sessions_sql};

    #[test]
    fn generate_reset_token_is_url_safe() {
        let token = generate_reset_token();
        assert!(!token.contains('+'));
        assert!(!token.contains('/'));
        assert!(!token.contains('='));
    }

    #[test]
    fn claim_token_sql_guards_against_reuse_and_expiry() {
        let sql = claim_token_sql();
        assert!(sql.contains("used_at IS NULL"));
        assert!(sql.contains("expires_at"));
        assert!(sql.contains("RETURNING id, user_id"));
    }

    #[test]
    fn revoke_user_sessions_sql_revokes_active_sessions() {
        let sql = revoke_user_sessions_sql();
        assert!(sql.contains("user_sessions"));
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("revoked_at"));
    }
}

/// Reset password using token
/// Validates token, updates password, marks token as used
async fn reset_password_inner(
    state: AppState,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<(ResetPasswordResponse, Uuid, Uuid), AppError> {
    // Hash the incoming token to compare with stored hash
    let mut hasher = Sha256::new();
    hasher.update(payload.token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());

    // Validate password strength (minimum 8 characters)
    if payload.new_password.len() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters long".to_string()));
    }

    // Hash new password
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(|_| AppError::Internal)?
        .to_string();

    // Update password and mark token as used in a transaction
    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;

    // Atomically claim the token inside the transaction to prevent races
    let token_row = sqlx::query(claim_token_sql())
    .bind(&Utc::now())
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    let (token_id, user_id): (Uuid, Uuid) = match token_row {
        Some(row) => (row.get("id"), row.get("user_id")),
        None => {
            // Generic error - don't reveal if token doesn't exist, expired, or already used
            return Err(AppError::BadRequest("Invalid or expired reset token".to_string()));
        }
    };

    let business_row = sqlx::query("SELECT business_id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;
    let business_id: Uuid = business_row.get("business_id");

    // Update user password
    sqlx::query(
        "UPDATE users SET password_hash = $1, updated_at = $2 WHERE id = $3"
    )
    .bind(&password_hash)
    .bind(&Utc::now())
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    // Invalidate all other reset tokens for this user (security: single-use)
    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = $1 WHERE user_id = $2 AND id != $3 AND used_at IS NULL"
    )
    .bind(&Utc::now())
    .bind(&user_id)
    .bind(&token_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    // Revoke all active sessions for this user to invalidate refresh tokens
    sqlx::query(revoke_user_sessions_sql())
    .bind(&Utc::now())
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    tx.commit().await.map_err(|_| AppError::Internal)?;

    tracing::info!("Password reset successful for user {}", user_id);

    Ok((
        ResetPasswordResponse {
            message: "Password has been reset successfully. You can now log in with your new password.".to_string(),
        },
        user_id,
        business_id,
    ))
}

pub async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, AppError> {
    let path = "/api/v1/auth/password-reset/reset";
    let mut meta = HashMap::new();
    match reset_password_inner(state.clone(), Json(payload)).await {
        Ok((body, uid, bid)) => {
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.password_reset.complete",
                bid,
                ActorType::User,
                &uid.to_string(),
                vec![],
                "user",
                uid,
                200,
                None,
                meta,
            )
            .await;
            Ok(Json(body))
        }
        Err(e) => {
            meta.insert(
                "http_status".into(),
                audit_emit::http_status_for_error(&e).to_string(),
            );
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.password_reset.complete",
                Uuid::nil(),
                ActorType::Anonymous,
                "",
                vec![],
                "user",
                Uuid::nil(),
                audit_emit::http_status_for_error(&e),
                Some(audit_emit::truncate_reason(&e.to_string())),
                meta,
            )
            .await;
            Err(e)
        }
    }
}

fn claim_token_sql() -> &'static str {
    "UPDATE password_reset_tokens
     SET used_at = $1
     WHERE token_hash = $2 AND used_at IS NULL AND expires_at >= $1
     RETURNING id, user_id"
}

fn revoke_user_sessions_sql() -> &'static str {
    "UPDATE user_sessions
     SET status = 'revoked', revoked_at = $1
     WHERE user_id = $2 AND status = 'active'"
}
