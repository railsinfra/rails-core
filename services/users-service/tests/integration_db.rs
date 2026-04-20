//! PostgreSQL-backed integration tests. Set `DATABASE_URL` (e.g. CI or local Postgres).

use axum::body::{to_bytes, Body};
use axum::extract::{FromRequestParts, State};
use axum::http::{header, Request, StatusCode};
use axum::Json;
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use tower::{Service, ServiceExt};
use uuid::Uuid;

use users_service::auth::{ApiKeyOnlyContext, AuthContext};
use users_service::db;
use users_service::error::AppError;
use users_service::grpc::GrpcClients;
use users_service::grpc_server::proto::users_service_server::UsersService;
use users_service::grpc_server::proto::ValidateApiKeyRequest;
use users_service::grpc_server::UsersGrpcService;
use users_service::routes::apikey::{
    create_api_key, list_api_keys, revoke_api_key, CreateApiKeyRequest,
};
use users_service::routes::auth::{
    login, refresh_token, revoke_token, LoginRequest, RefreshTokenRequest, RevokeTokenRequest,
};
use users_service::routes::beta::{apply_for_beta, BetaApplicationRequest};
use users_service::routes::business::{register_business, RegisterBusinessRequest};
use users_service::routes::password_reset::{
    request_password_reset, reset_password, RequestPasswordResetRequest, ResetPasswordRequest,
};
use users_service::routes::user::me;
use users_service::routes::{register_routes, AppState};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    users_service::test_support::global_test_lock()
}

async fn test_pool() -> Option<sqlx::PgPool> {
    let database_url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .connect(&database_url)
        .await
        .ok()?;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

fn grpc_none() -> GrpcClients {
    GrpcClients::none()
}

fn empty_request_parts() -> axum::http::request::Parts {
    Request::builder()
        .uri("/")
        .body(())
        .unwrap()
        .into_parts()
        .0
}

fn register_payload(email: &str) -> RegisterBusinessRequest {
    RegisterBusinessRequest {
        name: format!("Co {}", Uuid::new_v4()),
        website: None,
        admin_first_name: "Admin".into(),
        admin_last_name: "User".into(),
        admin_email: email.to_string(),
        admin_password: "password123!".into(),
    }
}

#[tokio::test]
async fn db_init_succeeds_when_database_url_valid() {
    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping db_init_succeeds_when_database_url_valid.");
            return;
        }
    };
    let url = std::env::var("DATABASE_URL").unwrap();
    let fresh = db::init(&url).await.expect("db::init");
    sqlx::query("SELECT 1")
        .fetch_one(&fresh)
        .await
        .expect("ping");
    drop(fresh);
    drop(pool);
}

#[tokio::test]
async fn register_login_me_refresh_revoke_api_keys_and_grpc_validate() {
    let _lock = env_lock();
    std::env::set_var("JWT_SECRET", "integration_test_jwt_secret");
    std::env::set_var("API_KEY_HASH_SECRET", "integration_test_api_key_hash");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping register_login_me_refresh_revoke_api_keys_and_grpc_validate.");
            return;
        }
    };

    let email = format!("int+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };

    let reg = register_business(State(state.clone()), Json(register_payload(&email)))
        .await
        .expect("register_business");
    let body = reg.0;
    let sandbox_id = body
        .environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox env")
        .id;
    let prod_id = body
        .environments
        .iter()
        .find(|e| e.r#type == "production")
        .expect("prod env")
        .id;

    let login_sandbox = login(
        State(state.clone()),
        Json(LoginRequest {
            email: email.clone(),
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login default sandbox");
    let access = login_sandbox.0.access_token.clone();
    let refresh = login_sandbox.0.refresh_token.clone();

    let wrong_pw = login(
        State(state.clone()),
        Json(LoginRequest {
            email: email.clone(),
            password: "wrong-password".into(),
            environment_id: None,
        }),
    )
    .await;
    assert!(wrong_pw.is_err());

    let mut parts = empty_request_parts();
    parts.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    parts
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx = AuthContext::from_request_parts(&mut parts, &state)
        .await
        .expect("jwt auth context");
    assert!(ctx.user_id.is_some());

    let me_resp = me(State(state.clone()), ctx.clone()).await.expect("/me");
    assert_eq!(me_resp.0.user.email, email.trim().to_lowercase());

    let mut parts_prod_me = empty_request_parts();
    parts_prod_me.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    parts_prod_me
        .headers
        .insert("x-environment-id", prod_id.to_string().parse().unwrap());
    let ctx_prod_me = AuthContext::from_request_parts(&mut parts_prod_me, &state)
        .await
        .expect("jwt ctx for production header");
    let me_prod = me(State(state.clone()), ctx_prod_me)
        .await
        .expect("/me with prod env");
    assert_eq!(me_prod.0.environment.r#type, "production");

    let mut jwt_bad = empty_request_parts();
    jwt_bad.headers.insert(
        header::AUTHORIZATION,
        "Bearer not-a-valid-jwt".parse().unwrap(),
    );
    jwt_bad
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    assert!(AuthContext::from_request_parts(&mut jwt_bad, &state)
        .await
        .is_err());

    let mut env_bad_uuid = empty_request_parts();
    env_bad_uuid.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    env_bad_uuid
        .headers
        .insert("x-environment-id", "not-a-uuid".parse().unwrap());
    assert!(matches!(
        AuthContext::from_request_parts(&mut env_bad_uuid, &state).await,
        Err(AppError::BadRequest(_))
    ));

    let key_resp = create_api_key(
        State(state.clone()),
        ctx,
        Json(CreateApiKeyRequest {
            environment_id: Some(sandbox_id),
        }),
    )
    .await
    .expect("create_api_key");
    let plain_key = key_resp.0.key.clone();

    let mut parts2 = empty_request_parts();
    parts2
        .headers
        .insert("x-api-key", plain_key.parse().unwrap());
    parts2
        .headers
        .insert("x-environment", "sandbox".parse().unwrap());
    let api_ctx = ApiKeyOnlyContext::from_request_parts(&mut parts2, &state)
        .await
        .expect("api key only context");
    assert_eq!(api_ctx.environment_id, sandbox_id);

    let mut parts_staging = empty_request_parts();
    parts_staging
        .headers
        .insert("x-api-key", plain_key.parse().unwrap());
    parts_staging
        .headers
        .insert("x-environment", "staging".parse().unwrap());
    let staging_err = ApiKeyOnlyContext::from_request_parts(&mut parts_staging, &state).await;
    assert!(matches!(staging_err, Err(AppError::BadRequest(_))));

    let mut jwt_parts_sbx = empty_request_parts();
    jwt_parts_sbx.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    jwt_parts_sbx
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx_sbx_admin = AuthContext::from_request_parts(&mut jwt_parts_sbx, &state)
        .await
        .expect("jwt admin in sandbox");
    let list = list_api_keys(State(state.clone()), ctx_sbx_admin.clone())
        .await
        .expect("list keys");
    assert!(!list.0.is_empty());

    let refreshed = refresh_token(
        State(state.clone()),
        Json(RefreshTokenRequest {
            refresh_token: refresh.clone(),
        }),
    )
    .await
    .expect("refresh");
    let bad_refresh = refresh_token(
        State(state.clone()),
        Json(RefreshTokenRequest {
            refresh_token: "not-a-real-token".into(),
        }),
    )
    .await;
    assert!(bad_refresh.is_err());

    let new_rt = refreshed.0.refresh_token.clone();
    let _ = revoke_token(
        State(state.clone()),
        Json(RevokeTokenRequest {
            refresh_token: new_rt.clone(),
        }),
    )
    .await
    .expect("revoke new refresh");

    let revoke_twice = revoke_token(
        State(state.clone()),
        Json(RevokeTokenRequest {
            refresh_token: new_rt,
        }),
    )
    .await;
    assert!(revoke_twice.is_err());

    let grpc = UsersGrpcService::new(pool.clone());
    let bad_grpc = grpc
        .validate_api_key(tonic::Request::new(ValidateApiKeyRequest {
            api_key: "not-a-real-api-key-value".into(),
            environment: "sandbox".into(),
        }))
        .await;
    assert!(bad_grpc.is_err());

    let validate = grpc
        .validate_api_key(tonic::Request::new(ValidateApiKeyRequest {
            api_key: plain_key.clone(),
            environment: "sandbox".into(),
        }))
        .await
        .expect("grpc validate");
    let inner = validate.into_inner();
    assert!(!inner.business_id.is_empty());

    let mut parts_bad_env = empty_request_parts();
    parts_bad_env
        .headers
        .insert("x-api-key", "totally-not-a-key".parse().unwrap());
    parts_bad_env
        .headers
        .insert("x-environment", "sandbox".parse().unwrap());
    let bad_key = ApiKeyOnlyContext::from_request_parts(&mut parts_bad_env, &state).await;
    assert!(bad_key.is_err());

    let mut jwt_ctx = empty_request_parts();
    jwt_ctx.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    jwt_ctx
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx_revoke = AuthContext::from_request_parts(&mut jwt_ctx, &state)
        .await
        .expect("ctx for revoke key");
    let bad_env_key = create_api_key(
        State(state.clone()),
        ctx_revoke,
        Json(CreateApiKeyRequest {
            environment_id: Some(Uuid::new_v4()),
        }),
    )
    .await;
    assert!(matches!(bad_env_key, Err(AppError::BadRequest(_))));

    let mut jwt_ctx2 = empty_request_parts();
    jwt_ctx2.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    jwt_ctx2
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx_r = AuthContext::from_request_parts(&mut jwt_ctx2, &state)
        .await
        .unwrap();
    let _ = revoke_api_key(State(state.clone()), ctx_r, axum::extract::Path(key_resp.0.id))
        .await
        .expect("revoke api key");

    let revoked_grpc = grpc
        .validate_api_key(tonic::Request::new(ValidateApiKeyRequest {
            api_key: plain_key.clone(),
            environment: "sandbox".into(),
        }))
        .await;
    assert!(revoked_grpc.is_err());

    std::env::remove_var("JWT_SECRET");
    std::env::remove_var("API_KEY_HASH_SECRET");
}

#[tokio::test]
async fn register_business_rejects_empty_admin_email() {
    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping register_business_rejects_empty_admin_email.");
            return;
        }
    };
    let state = AppState {
        db: pool,
        grpc: grpc_none(),
        email: None,
    };
    let mut req = register_payload(&format!("x+{}@example.com", Uuid::new_v4()));
    req.admin_email = "   ".into();
    let err = register_business(State(state), Json(req))
        .await
        .expect_err("empty email");
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn password_reset_happy_and_failure_paths() {
    let _lock = env_lock();
    std::env::set_var("JWT_SECRET", "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping password_reset_happy_and_failure_paths.");
            return;
        }
    };

    let email = format!("reset+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };
    let _ = register_business(State(state.clone()), Json(register_payload(&email)))
        .await
        .expect("register");

    let _ = request_password_reset(
        State(state.clone()),
        Json(RequestPasswordResetRequest {
            email: email.clone(),
        }),
    )
    .await
    .expect("reset request for real user");

    let unknown = request_password_reset(
        State(state.clone()),
        Json(RequestPasswordResetRequest {
            email: "nobody-here@example.com".into(),
        }),
    )
    .await
    .expect("unknown email still 200");
    assert!(unknown.0.message.contains("If an account exists"));

    let short_pw = reset_password(
        State(state.clone()),
        Json(ResetPasswordRequest {
            token: "any".into(),
            new_password: "short".into(),
        }),
    )
    .await;
    assert!(short_pw.is_err());

    let bad_tok = reset_password(
        State(state.clone()),
        Json(ResetPasswordRequest {
            token: "not-valid-token-xxxxxxxxxxxxxxxx".into(),
            new_password: "longenough1!".into(),
        }),
    )
    .await;
    assert!(bad_tok.is_err());

    std::env::remove_var("JWT_SECRET");
}

#[tokio::test]
async fn beta_apply_conflict_on_duplicate_email() {
    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping beta_apply_conflict_on_duplicate_email.");
            return;
        }
    };
    let state = AppState {
        db: pool,
        grpc: grpc_none(),
        email: None,
    };
    let mail = format!("beta-dup+{}@example.com", Uuid::new_v4());
    let payload1 = BetaApplicationRequest {
        name: "A".into(),
        email: mail.clone(),
        company: "C".into(),
        use_case: "U".into(),
    };
    let payload2 = BetaApplicationRequest {
        name: "A".into(),
        email: mail,
        company: "C".into(),
        use_case: "U".into(),
    };
    let _ = apply_for_beta(State(state.clone()), Json(payload1))
        .await
        .expect("first beta");
    let second = apply_for_beta(State(state), Json(payload2)).await;
    assert!(matches!(second, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn http_router_health_and_correlation_header() {
    let _lock = env_lock();
    std::env::set_var("JWT_SECRET", "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping http_router_health_and_correlation_header.");
            return;
        }
    };

    let app = register_routes(pool, grpc_none(), None);
    let mut svc = app.into_service();

    let health = svc
        .ready()
        .await
        .unwrap()
        .call(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let api = svc
        .ready()
        .await
        .unwrap()
        .call(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/revoke")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "refresh_token": "nope" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::BAD_REQUEST);
    assert!(api.headers().get("x-correlation-id").is_some());

    let bytes = to_bytes(api.into_body(), 1024 * 64).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("error").is_some() || v.get("message").is_some());

    std::env::remove_var("JWT_SECRET");
}

#[tokio::test]
async fn internal_service_token_blocks_sensitive_routes_when_configured() {
    let _lock = env_lock();
    std::env::set_var("INTERNAL_SERVICE_TOKEN_ALLOWLIST", "secret-one,secret-two");
    std::env::set_var("JWT_SECRET", "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping internal_service_token_blocks_sensitive_routes_when_configured.");
            return;
        }
    };

    let app = register_routes(pool, grpc_none(), None);
    let mut svc = app.into_service();

    let blocked = svc
        .ready()
        .await
        .unwrap()
        .call(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);

    let ok = svc
        .ready()
        .await
        .unwrap()
        .call(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-internal-service-token", "secret-one")
                .body(Body::from(
                    json!({ "email": "x", "password": "y" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(ok.status(), StatusCode::FORBIDDEN);

    std::env::remove_var("INTERNAL_SERVICE_TOKEN_ALLOWLIST");
    std::env::remove_var("JWT_SECRET");
}

#[tokio::test]
async fn password_reset_completes_with_seeded_token() {
    let _lock = env_lock();
    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping password_reset_completes_with_seeded_token.");
            return;
        }
    };

    let email = format!("seed-reset+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };
    let _ = register_business(State(state.clone()), Json(register_payload(&email)))
        .await
        .expect("register");

    let email_norm = email.trim().to_lowercase();
    let user_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE email = $1 AND status = 'active' LIMIT 1",
    )
    .bind(&email_norm)
    .fetch_one(&pool)
    .await
    .expect("user id");

    let raw_token = "integration-reset-token-32chars!!";
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    let token_id = Uuid::new_v4();
    let expires_at = Utc::now() + chrono::Duration::hours(1);
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(token_id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .bind(now)
    .execute(&pool)
    .await
    .expect("insert token");

    let done = reset_password(
        State(state.clone()),
        Json(ResetPasswordRequest {
            token: raw_token.into(),
            new_password: "newpassword1!".into(),
        }),
    )
    .await
    .expect("reset password");
    assert!(done.0.message.to_lowercase().contains("success"));

    let login_ok = login(
        State(state),
        Json(LoginRequest {
            email,
            password: "newpassword1!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login with new password");
    assert!(!login_ok.0.access_token.is_empty());
}
