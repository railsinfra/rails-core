use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use uuid::Uuid;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::Row;

use crate::error::AppError;
use crate::routes::AppState;

pub const ENVIRONMENT_ID_HEADER: &str = "X-Environment-Id";
pub const API_KEY_HEADER: &str = "X-API-Key";
pub const ENVIRONMENT_HEADER: &str = "X-Environment";

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub business_id: Uuid,
    pub environment_id: Uuid,
}

/// SDK-only auth: API key + X-Environment. No JWT. Use for routes called exclusively by SDKs (e.g. POST/GET /api/v1/users).
#[derive(Clone, Debug)]
pub struct ApiKeyOnlyContext {
    pub api_key_id: Uuid,
    pub business_id: Uuid,
    pub environment_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    sub: String,
    #[allow(dead_code)]
    exp: i64, // Used by jsonwebtoken for validation, but not explicitly read
    #[allow(dead_code)]
    env: Option<String>, // Reserved for future use
}

#[async_trait]
impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let environment_id_from_uuid_header: Option<Uuid> = parts
            .headers
            .get(ENVIRONMENT_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|raw| {
                Uuid::parse_str(&raw)
                    .map_err(|_| AppError::BadRequest(format!("Invalid {} header", ENVIRONMENT_ID_HEADER)))
            })
            .transpose()?;

        let environment_from_string_header: Option<String> = parts
            .headers
            .get(ENVIRONMENT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(api_key_plain) = parts
            .headers
            .get(API_KEY_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            let now = Utc::now();
            let key_hash = hash_api_key(&api_key_plain)?;

            let rec = sqlx::query(
                "SELECT k.id, k.business_id, k.revoked_at, k.status FROM api_keys k WHERE k.key_hash = $1"
            )
            .bind(&key_hash)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::Unauthorized)?;

            let api_key_id: Uuid = rec.try_get("id").map_err(|_| AppError::Internal)?;
            let business_id: Uuid = rec.try_get("business_id").map_err(|_| AppError::Internal)?;
            let status: String = rec.try_get("status").map_err(|_| AppError::Internal)?;
            let revoked_at: Option<chrono::DateTime<Utc>> = rec.try_get("revoked_at").map_err(|_| AppError::Internal)?;

            if status != "active" || revoked_at.is_some() {
                return Err(AppError::Unauthorized);
            }

            let environment_id = if let Some(env_id) = environment_id_from_uuid_header {
                env_id
            } else {
                let env = environment_from_string_header
                    .ok_or_else(|| AppError::BadRequest(format!("Missing {} header", ENVIRONMENT_HEADER)))?;

                if env != "sandbox" && env != "production" {
                    return Err(AppError::BadRequest(format!(
                        "Invalid {} header: must be 'sandbox' or 'production'",
                        ENVIRONMENT_HEADER
                    )));
                }

                let env_rec = sqlx::query(
                    "SELECT id FROM environments WHERE business_id = $1 AND type = $2 AND status = 'active'"
                )
                .bind(&business_id)
                .bind(&env)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| AppError::Internal)?
                .ok_or_else(|| AppError::Forbidden)?;

                env_rec.try_get("id").map_err(|_| AppError::Internal)?
            };

            let env_ok = sqlx::query(
                "SELECT 1 FROM environments WHERE id = $1 AND business_id = $2 AND status = 'active'"
            )
            .bind(&environment_id)
            .bind(&business_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;

            if env_ok.is_none() {
                return Err(AppError::Forbidden);
            }

            let _ = sqlx::query("UPDATE api_keys SET last_used_at = $1 WHERE id = $2")
                .bind(&now)
                .bind(&api_key_id)
                .execute(&state.db)
                .await;

            return Ok(Self {
                user_id: None,
                api_key_id: Some(api_key_id),
                business_id,
                environment_id,
            });
        }

        let environment_id = environment_id_from_uuid_header
            .ok_or_else(|| AppError::BadRequest(format!("Missing {} header", ENVIRONMENT_ID_HEADER)))?;

        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev_secret".to_string());
        let decoded = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized)?;

        let user_id = Uuid::parse_str(&decoded.claims.sub).map_err(|_| AppError::Unauthorized)?;

        // First, try to find user in the requested environment
        let rec = sqlx::query(
            "SELECT business_id FROM users WHERE id = $1 AND environment_id = $2 AND status = 'active'"
        )
        .bind(&user_id)
        .bind(&environment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

        let business_id: Uuid = if let Some(row) = rec {
            // User exists in requested environment
            row.try_get("business_id").map_err(|_| AppError::Internal)?
        } else {
            // User doesn't exist in requested environment, but check if they exist in any environment for the same business
            // This allows admins to access both sandbox and production even if they only have a user record in one
            let any_user_rec = sqlx::query(
                "SELECT business_id FROM users WHERE id = $1 AND status = 'active' LIMIT 1"
            )
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::Forbidden)?;
            
            let user_business_id: Uuid = any_user_rec.try_get("business_id").map_err(|_| AppError::Internal)?;
            
            // Verify that the requested environment_id belongs to the same business
            let env_check = sqlx::query(
                "SELECT 1 FROM environments WHERE id = $1 AND business_id = $2 AND status = 'active'"
            )
            .bind(&environment_id)
            .bind(&user_business_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;
            
            if env_check.is_none() {
                return Err(AppError::Forbidden);
            }
            
            user_business_id
        };

        Ok(Self {
            user_id: Some(user_id),
            api_key_id: None,
            business_id,
            environment_id,
        })
    }
}

#[async_trait]
impl FromRequestParts<AppState> for ApiKeyOnlyContext {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // #region agent log
        let has_api_key_header = parts.headers.get(API_KEY_HEADER).is_some();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/Users/sibabale.joja/projects/personal/rails/.cursor/debug.log") {
            let _ = std::io::Write::write_fmt(&mut f, format_args!("{{\"location\":\"auth.rs:ApiKeyOnlyContext\",\"message\":\"entry\",\"data\":{{\"has_x_api_key_header\":{}}},\"timestamp\":{},\"hypothesisId\":\"C\"}}\n", has_api_key_header, chrono::Utc::now().timestamp_millis()));
        }
        // #endregion
        let environment_id_from_uuid_header: Option<Uuid> = parts
            .headers
            .get(ENVIRONMENT_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|raw| {
                Uuid::parse_str(&raw)
                    .map_err(|_| AppError::BadRequest(format!("Invalid {} header", ENVIRONMENT_ID_HEADER)))
            })
            .transpose()?;

        let environment_from_string_header: Option<String> = parts
            .headers
            .get(ENVIRONMENT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let api_key_plain = parts
            .headers
            .get(API_KEY_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or(AppError::Unauthorized)?;

        let now = Utc::now();
        let key_hash = hash_api_key(&api_key_plain)?;

        let rec = sqlx::query(
            "SELECT k.id, k.business_id, k.revoked_at, k.status FROM api_keys k WHERE k.key_hash = $1"
        )
        .bind(&key_hash)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

        // #region agent log
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/Users/sibabale.joja/projects/personal/rails/.cursor/debug.log") {
            let _ = std::io::Write::write_fmt(&mut f, format_args!("{{\"location\":\"auth.rs:ApiKeyOnlyContext\",\"message\":\"after DB lookup\",\"data\":{{\"key_found_in_db\":{}}},\"timestamp\":{},\"hypothesisId\":\"D\"}}\n", rec.is_some(), chrono::Utc::now().timestamp_millis()));
        }
        // #endregion

        let rec = rec.ok_or(AppError::Unauthorized)?;

        let api_key_id: Uuid = rec.try_get("id").map_err(|_| AppError::Internal)?;
        let business_id: Uuid = rec.try_get("business_id").map_err(|_| AppError::Internal)?;
        let status: String = rec.try_get("status").map_err(|_| AppError::Internal)?;
        let revoked_at: Option<chrono::DateTime<Utc>> = rec.try_get("revoked_at").map_err(|_| AppError::Internal)?;

        if status != "active" || revoked_at.is_some() {
            return Err(AppError::Unauthorized);
        }

        let environment_id = if let Some(env_id) = environment_id_from_uuid_header {
            env_id
        } else {
            let env = environment_from_string_header
                .ok_or_else(|| AppError::BadRequest(format!("Missing {} header", ENVIRONMENT_HEADER)))?;

            if env != "sandbox" && env != "production" {
                return Err(AppError::BadRequest(format!(
                    "Invalid {} header: must be 'sandbox' or 'production'",
                    ENVIRONMENT_HEADER
                )));
            }

            let env_rec = sqlx::query(
                "SELECT id FROM environments WHERE business_id = $1 AND type = $2 AND status = 'active'"
            )
            .bind(&business_id)
            .bind(&env)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or_else(|| AppError::Forbidden)?;

            env_rec.try_get("id").map_err(|_| AppError::Internal)?
        };

        let env_ok = sqlx::query(
            "SELECT 1 FROM environments WHERE id = $1 AND business_id = $2 AND status = 'active'"
        )
        .bind(&environment_id)
        .bind(&business_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

        if env_ok.is_none() {
            return Err(AppError::Forbidden);
        }

        let _ = sqlx::query("UPDATE api_keys SET last_used_at = $1 WHERE id = $2")
            .bind(&now)
            .bind(&api_key_id)
            .execute(&state.db)
            .await;

        Ok(Self {
            api_key_id,
            business_id,
            environment_id,
        })
    }
}

pub(crate) fn hash_api_key(api_key_plain: &str) -> Result<String, AppError> {
    let secret = std::env::var("API_KEY_HASH_SECRET")
        .unwrap_or_else(|_| "dev_api_key_hash_secret".to_string());
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Internal)?;
    mac.update(api_key_plain.as_bytes());
    let out = mac.finalize().into_bytes();
    Ok(hex_encode(&out))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
