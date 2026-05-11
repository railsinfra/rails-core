use std::collections::HashMap;
use std::net::SocketAddr;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::audit_emit;
use crate::auth::{ApiKeyOnlyContext, AuthContext};
use crate::error::{AppError, DUPLICATE_EMAIL_MESSAGE};
use crate::grpc::audit_proto::ActorType;
use crate::routes::AppState;

/// Normalize email for storage and lookup: trim and lowercase.
pub(crate) fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

const SDK_USER_MIN_PASSWORD_LEN: usize = 8;

/// Validates SDK `POST /api/v1/users` body (trimmed names, normalized email, password length).
pub(crate) fn validate_sdk_user_payload(
    email: &str,
    first_name: &str,
    last_name: &str,
    password: &str,
) -> Result<(String, String, String), AppError> {
    let email = normalize_email(email);
    if email.is_empty() {
        return Err(AppError::BadRequest("Email is required.".to_string()));
    }
    let first_name = first_name.trim().to_string();
    if first_name.is_empty() {
        return Err(AppError::BadRequest("First name is required.".to_string()));
    }
    let last_name = last_name.trim().to_string();
    if last_name.is_empty() {
        return Err(AppError::BadRequest("Last name is required.".to_string()));
    }
    if password.len() < SDK_USER_MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "Password must be at least {SDK_USER_MIN_PASSWORD_LEN} characters."
        )));
    }
    Ok((email, first_name, last_name))
}

#[derive(Deserialize)]
pub struct CreateSdkUserRequest {
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct CreateSdkUserResponse {
    pub status: String,
    pub user_id: Uuid,
}

async fn create_sdk_user_inner(
    state: AppState,
    ctx: ApiKeyOnlyContext,
    Json(payload): Json<CreateSdkUserRequest>,
) -> Result<CreateSdkUserResponse, AppError> {
    let (email, first_name, last_name) = validate_sdk_user_payload(
        &payload.email,
        &payload.first_name,
        &payload.last_name,
        &payload.password,
    )?;

    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    if exists {
        return Err(AppError::Conflict(DUPLICATE_EMAIL_MESSAGE.to_string()));
    }

    let user_id = Uuid::new_v4();
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|_| AppError::Internal)?
        .to_string();

    sqlx::query(
        r#"INSERT INTO users (
            id, business_id, environment_id, first_name, last_name, email, password_hash,
            role, status, created_at, updated_at, created_by_user_id, created_by_api_key_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'user', 'active', NOW(), NOW(), NULL, $8)"#,
    )
    .bind(&user_id)
    .bind(&ctx.business_id)
    .bind(&ctx.environment_id)
    .bind(&first_name)
    .bind(&last_name)
    .bind(&email)
    .bind(&password_hash)
    .bind(&ctx.api_key_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error() {
            if db_err.message().contains("unique_email") {
                return AppError::Conflict(DUPLICATE_EMAIL_MESSAGE.to_string());
            }
        }
        AppError::Internal
    })?;

    Ok(CreateSdkUserResponse {
        status: "active".to_string(),
        user_id,
    })
}

pub async fn create_sdk_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    ctx: ApiKeyOnlyContext,
    Json(payload): Json<CreateSdkUserRequest>,
) -> Result<Json<CreateSdkUserResponse>, AppError> {
    const PATH: &str = "/api/v1/users";
    const ACTION: &str = "users.sdk.user.create";
    let business_id = ctx.business_id;
    let api_key_id = ctx.api_key_id;
    let out = create_sdk_user_inner(state.clone(), ctx, Json(payload)).await;
    let mut meta = HashMap::new();
    meta.insert("api_key_id".to_string(), api_key_id.to_string());
    match &out {
        Ok(body) => {
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                PATH,
                ACTION,
                business_id,
                ActorType::Anonymous,
                "",
                vec![],
                "user",
                body.user_id,
                200,
                None,
                meta,
            )
            .await;
        }
        Err(e) => {
            meta.insert(
                "http_status".into(),
                audit_emit::http_status_for_error(e).to_string(),
            );
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                PATH,
                ACTION,
                business_id,
                ActorType::Anonymous,
                "",
                vec![],
                "user",
                Uuid::nil(),
                audit_emit::http_status_for_error(e),
                Some(audit_emit::truncate_reason(&e.to_string())),
                meta,
            )
            .await;
        }
    }
    out.map(Json)
}

#[derive(Serialize)]
pub struct MeUser {
    pub id: Uuid,
    pub business_id: Uuid,
    pub environment_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub role: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct MeBusiness {
    pub id: Uuid,
    /// Canonical org identifier for Accounts/Ledger; equals business id.
    pub organization_id: Uuid,
    pub name: String,
    pub website: Option<String>,
    pub status: String,
}

#[derive(Serialize)]
pub struct MeEnvironment {
    pub id: Uuid,
    pub business_id: Uuid,
    pub r#type: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub user: MeUser,
    pub business: MeBusiness,
    pub environment: MeEnvironment,
}

pub async fn me(
    State(state): State<AppState>,
    ctx: AuthContext,
) -> Result<Json<MeResponse>, AppError> {
    let user_id = ctx.user_id.ok_or(AppError::Forbidden)?;
    
    // First, try to find user in the requested environment
    let user_row = sqlx::query(
        "SELECT id, business_id, environment_id, first_name, last_name, email, role, status FROM users WHERE id = $1 AND environment_id = $2 AND status = 'active'"
    )
    .bind(&user_id)
    .bind(&ctx.environment_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    
    // If user doesn't exist in requested environment, find them in any environment for the same business
    // This allows users to access both sandbox and production even if they only have a user record in one
    let user_row = if let Some(row) = user_row {
        Some(row)
    } else {
        // Find user in any environment for the same business
        let any_user_row = sqlx::query(
            "SELECT id, business_id, environment_id, first_name, last_name, email, role, status FROM users WHERE id = $1 AND business_id = $2 AND status = 'active' LIMIT 1"
        )
        .bind(&user_id)
        .bind(&ctx.business_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
        
        // Verify that the requested environment_id belongs to the same business
        if any_user_row.is_some() {
            let env_check = sqlx::query(
                "SELECT 1 FROM environments WHERE id = $1 AND business_id = $2 AND status = 'active'"
            )
            .bind(&ctx.environment_id)
            .bind(&ctx.business_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;
            
            if env_check.is_none() {
                return Err(AppError::Forbidden);
            }
        }
        
        any_user_row
    };
    
    let user_row = user_row.ok_or(AppError::Forbidden)?;

    let user = MeUser {
        id: user_row.get("id"),
        business_id: user_row.get("business_id"),
        environment_id: user_row.get("environment_id"),
        first_name: user_row.get("first_name"),
        last_name: user_row.get("last_name"),
        email: user_row.get("email"),
        role: user_row.get("role"),
        status: user_row.get("status"),
    };

    let env_row = sqlx::query(
        "SELECT id, business_id, type, status FROM environments WHERE id = $1 AND status = 'active'"
    )
    .bind(&ctx.environment_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::BadRequest("Invalid environment_id".to_string()))?;

    let environment = MeEnvironment {
        id: env_row.get("id"),
        business_id: env_row.get("business_id"),
        r#type: env_row.get("type"),
        status: env_row.get("status"),
    };

    let business_row = sqlx::query(
        "SELECT id, name, website, status FROM businesses WHERE id = $1 AND status = 'active'"
    )
    .bind(&user.business_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::BadRequest("Invalid business_id".to_string()))?;

    let business_id: Uuid = business_row.get("id");
    let business = MeBusiness {
        id: business_id,
        organization_id: business_id,
        name: business_row.get("name"),
        website: business_row.get("website"),
        status: business_row.get("status"),
    };

    Ok(Json(MeResponse {
        user,
        business,
        environment,
    }))
}

#[cfg(test)]
mod tests {
    use super::{normalize_email, validate_sdk_user_payload};
    use crate::error::{AppError, DUPLICATE_EMAIL_MESSAGE};

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(normalize_email("  User@EXAMPLE.com \t"), "user@example.com");
    }

    #[test]
    fn duplicate_email_message_is_user_friendly_and_stable() {
        assert!(
            DUPLICATE_EMAIL_MESSAGE.contains("account") && DUPLICATE_EMAIL_MESSAGE.contains("email"),
            "Message should be non-technical and actionable"
        );
        assert!(
            DUPLICATE_EMAIL_MESSAGE.contains("signing in") || DUPLICATE_EMAIL_MESSAGE.contains("reset"),
            "Message should suggest sign in or password reset"
        );
    }

    #[test]
    fn validate_sdk_user_payload_rejects_empty_email() {
        let e = validate_sdk_user_payload("  ", "A", "B", "password123!").unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)));
    }

    #[test]
    fn validate_sdk_user_payload_rejects_empty_first_name() {
        let e = validate_sdk_user_payload("a@b.com", "  ", "B", "password123!").unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)));
    }

    #[test]
    fn validate_sdk_user_payload_rejects_empty_last_name() {
        let e = validate_sdk_user_payload("a@b.com", "A", "\t", "password123!").unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)));
    }

    #[test]
    fn validate_sdk_user_payload_rejects_short_password() {
        let e = validate_sdk_user_payload("a@b.com", "A", "B", "short7!").unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)));
    }

    #[test]
    fn validate_sdk_user_payload_accepts_valid_input() {
        let (em, f, l) =
            validate_sdk_user_payload("  User@EX.com ", "  Pat ", " Lee ", "password123!").unwrap();
        assert_eq!(em, "user@ex.com");
        assert_eq!(f, "Pat");
        assert_eq!(l, "Lee");
    }
}
