//! PostgreSQL-backed integration tests. Set `DATABASE_URL` (e.g. CI or local Postgres).

use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, FromRequestParts, Path, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Json;
use chrono::Utc;
use httpmock::prelude::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Endpoint, Server};
use tower::{Service, ServiceExt};
use uuid::Uuid;

use users_service::auth::{ApiKeyOnlyContext, AuthContext};
use users_service::config::Config;
use users_service::db;
use users_service::email::EmailService;
use users_service::error::AppError;
use users_service::grpc::audit_proto::audit_service_client::AuditServiceClient;
use users_service::grpc::audit_proto::audit_service_server::{AuditService, AuditServiceServer};
use users_service::grpc::audit_proto::{AppendAuditEventRequest, AppendAuditEventResponse};
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
use users_service::routes::business::{register_business, RegisterBusinessRequest, RegisterBusinessResponse};
use users_service::routes::password_reset::{
    request_password_reset, reset_password, RequestPasswordResetRequest, ResetPasswordRequest,
};
use users_service::routes::user::{create_sdk_user, me, CreateSdkUserRequest};
use users_service::routes::{register_routes, AppState};
use users_service::test_support::test_connect_info;

/// Environment keys as `const` values (not string literals at `set_var` / `remove_var` / `var` sites).
/// Matches production style and satisfies static analyzers that flag raw literals (e.g. DeepSource RS-W1015).
const DATABASE_URL_ENV: &str = "DATABASE_URL";
const JWT_SECRET_ENV: &str = "JWT_SECRET";
const API_KEY_HASH_SECRET_ENV: &str = "API_KEY_HASH_SECRET";
const INTERNAL_SERVICE_TOKEN_ALLOWLIST_ENV: &str = "INTERNAL_SERVICE_TOKEN_ALLOWLIST";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    users_service::test_support::global_test_lock()
}

async fn test_pool() -> Option<sqlx::PgPool> {
    let database_url = std::env::var(DATABASE_URL_ENV).ok()?;
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

struct FailingAuditGrpc;

#[tonic::async_trait]
impl AuditService for FailingAuditGrpc {
    async fn append_audit_event(
        &self,
        _request: tonic::Request<AppendAuditEventRequest>,
    ) -> Result<tonic::Response<AppendAuditEventResponse>, tonic::Status> {
        Err(tonic::Status::unavailable("audit unavailable for coverage test"))
    }
}

async fn grpc_clients_with_failing_audit() -> (
    GrpcClients,
    tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    tokio::sync::oneshot::Sender<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve = Server::builder()
        .add_service(AuditServiceServer::new(FailingAuditGrpc))
        .serve_with_incoming_shutdown(incoming, async {
            let _ = shutdown_rx.await;
        });
    let join = tokio::spawn(serve);
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    let ch = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect_lazy();
    let client = AuditServiceClient::new(ch);
    (GrpcClients::new(None, Some(client)), join, shutdown_tx)
}

fn hdr_empty() -> HeaderMap {
    HeaderMap::new()
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
    let url = std::env::var(DATABASE_URL_ENV).unwrap();
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
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");
    std::env::set_var(API_KEY_HASH_SECRET_ENV, "integration_test_api_key_hash");

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

    let reg = register_business(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
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
        HeaderMap::new(),
        test_connect_info(),
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
        HeaderMap::new(),
        test_connect_info(),
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
        HeaderMap::new(),
        test_connect_info(),
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
        HeaderMap::new(),
        test_connect_info(),
        Json(RefreshTokenRequest {
            refresh_token: refresh.clone(),
        }),
    )
    .await
    .expect("refresh");
    let bad_refresh = refresh_token(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(RefreshTokenRequest {
            refresh_token: "not-a-real-token".into(),
        }),
    )
    .await;
    assert!(bad_refresh.is_err());

    let new_rt = refreshed.0.refresh_token.clone();
    let _ = revoke_token(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(RevokeTokenRequest {
            refresh_token: new_rt.clone(),
        }),
    )
    .await
    .expect("revoke new refresh");

    let revoke_twice = revoke_token(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
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
        HeaderMap::new(),
        test_connect_info(),
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
    let _ = revoke_api_key(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        ctx_r,
        axum::extract::Path(key_resp.0.id),
    )
    .await
    .expect("revoke api key");

    let revoked_grpc = grpc
        .validate_api_key(tonic::Request::new(ValidateApiKeyRequest {
            api_key: plain_key.clone(),
            environment: "sandbox".into(),
        }))
        .await;
    assert!(revoked_grpc.is_err());

    std::env::remove_var(JWT_SECRET_ENV);
    std::env::remove_var(API_KEY_HASH_SECRET_ENV);
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
    let err = register_business(State(state), hdr_empty(), test_connect_info(), Json(req))
        .await
        .expect_err("empty email");
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn password_reset_happy_and_failure_paths() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

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
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");

    let _ = request_password_reset(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RequestPasswordResetRequest {
            email: email.clone(),
        }),
    )
    .await
    .expect("reset request for real user");

    let unknown = request_password_reset(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RequestPasswordResetRequest {
            email: "nobody-here@example.com".into(),
        }),
    )
    .await
    .expect("unknown email still 200");
    assert!(unknown.0.message.contains("If an account exists"));

    let short_pw = reset_password(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(ResetPasswordRequest {
            token: "any".into(),
            new_password: "short".into(),
        }),
    )
    .await;
    assert!(short_pw.is_err());

    let bad_tok = reset_password(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(ResetPasswordRequest {
            token: "not-valid-token-xxxxxxxxxxxxxxxx".into(),
            new_password: "longenough1!".into(),
        }),
    )
    .await;
    assert!(bad_tok.is_err());

    std::env::remove_var(JWT_SECRET_ENV);
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
    let _ = apply_for_beta(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(payload1),
    )
    .await
    .expect("first beta");
    let second = apply_for_beta(
        State(state),
        hdr_empty(),
        test_connect_info(),
        Json(payload2),
    )
    .await;
    assert!(matches!(second, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn sdk_create_user_via_api_key_login_duplicate_and_validation() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");
    std::env::set_var(API_KEY_HASH_SECRET_ENV, "integration_test_api_key_hash");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!(
                "DATABASE_URL not set; skipping sdk_create_user_via_api_key_login_duplicate_and_validation."
            );
            return;
        }
    };

    let admin_email = format!("admin+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };

    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&admin_email)),
    )
    .await
    .expect("register_business");

    let login_admin = login(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(LoginRequest {
            email: admin_email.clone(),
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login admin");
    let access = login_admin.0.access_token.clone();
    let sandbox_id = login_admin
        .0
        .environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox environment")
        .id;

    let mut jwt_parts = empty_request_parts();
    jwt_parts.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", access).parse().unwrap(),
    );
    jwt_parts
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx_admin = AuthContext::from_request_parts(&mut jwt_parts, &state)
        .await
        .expect("admin jwt context");

    let key_resp = create_api_key(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        ctx_admin,
        Json(CreateApiKeyRequest {
            environment_id: Some(sandbox_id),
        }),
    )
    .await
    .expect("create_api_key");
    let plain_key = key_resp.0.key.clone();

    let mut parts = empty_request_parts();
    parts
        .headers
        .insert("x-api-key", plain_key.parse().unwrap());
    parts
        .headers
        .insert("x-environment", "sandbox".parse().unwrap());
    let api_ctx = ApiKeyOnlyContext::from_request_parts(&mut parts, &state)
        .await
        .expect("api key only context");

    let sdk_email = format!("sdk+{}@example.com", Uuid::new_v4());
    let created = create_sdk_user(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        api_ctx.clone(),
        Json(CreateSdkUserRequest {
            email: sdk_email.clone(),
            first_name: "Sam".into(),
            last_name: "Sdk".into(),
            password: "password123!".into(),
        }),
    )
    .await
    .expect("create sdk user");
    assert_eq!(created.0.status, "active");
    assert_ne!(created.0.user_id, Uuid::nil());

    let _ = login(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(LoginRequest {
            email: sdk_email.clone(),
            password: "password123!".into(),
            environment_id: Some(sandbox_id),
        }),
    )
    .await
    .expect("sdk user login");

    let dup = create_sdk_user(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        api_ctx.clone(),
        Json(CreateSdkUserRequest {
            email: sdk_email.clone(),
            first_name: "Other".into(),
            last_name: "Person".into(),
            password: "password123!".into(),
        }),
    )
    .await;
    assert!(matches!(dup, Err(AppError::Conflict(_))));

    let dup_admin = create_sdk_user(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        api_ctx.clone(),
        Json(CreateSdkUserRequest {
            email: admin_email.clone(),
            first_name: "Dup".into(),
            last_name: "Admin".into(),
            password: "password123!".into(),
        }),
    )
    .await;
    assert!(matches!(dup_admin, Err(AppError::Conflict(_))));

    let short_pw = create_sdk_user(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        api_ctx.clone(),
        Json(CreateSdkUserRequest {
            email: format!("short+{}@example.com", Uuid::new_v4()),
            first_name: "A".into(),
            last_name: "B".into(),
            password: "short7!".into(),
        }),
    )
    .await;
    assert!(matches!(short_pw, Err(AppError::BadRequest(_))));

    let empty_fn = create_sdk_user(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        api_ctx,
        Json(CreateSdkUserRequest {
            email: format!("fn+{}@example.com", Uuid::new_v4()),
            first_name: "   ".into(),
            last_name: "B".into(),
            password: "password123!".into(),
        }),
    )
    .await;
    assert!(matches!(empty_fn, Err(AppError::BadRequest(_))));

    std::env::remove_var(JWT_SECRET_ENV);
    std::env::remove_var(API_KEY_HASH_SECRET_ENV);
}

#[tokio::test]
async fn http_router_health_and_correlation_header() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping http_router_health_and_correlation_header.");
            return;
        }
    };

    let app = register_routes(pool, grpc_none(), None);
    let mut svc = app.into_service();

    let peer = std::net::SocketAddr::from(([127, 0, 0, 1], 9));
    let mut health_req = Request::builder().uri("/health").body(Body::empty()).unwrap();
    health_req.extensions_mut().insert(ConnectInfo(peer));
    let health = svc.ready().await.unwrap().call(health_req).await.unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let mut api_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/revoke")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "refresh_token": "nope" }).to_string(),
        ))
        .unwrap();
    api_req.extensions_mut().insert(ConnectInfo(peer));
    let api = svc.ready().await.unwrap().call(api_req).await.unwrap();
    assert_eq!(api.status(), StatusCode::BAD_REQUEST);
    assert!(api.headers().get("x-correlation-id").is_some());

    let bytes = to_bytes(api.into_body(), 1024 * 64).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("error").is_some() || v.get("message").is_some());

    let mut users_req = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-environment", "sandbox")
        .body(Body::from(
            json!({
                "email": "router+nokey@example.com",
                "first_name": "A",
                "last_name": "B",
                "password": "password123!"
            })
            .to_string(),
        ))
        .unwrap();
    users_req.extensions_mut().insert(ConnectInfo(peer));
    let users = svc.ready().await.unwrap().call(users_req).await.unwrap();
    assert_eq!(users.status(), StatusCode::UNAUTHORIZED);

    std::env::remove_var(JWT_SECRET_ENV);
}

#[tokio::test]
async fn internal_service_token_blocks_sensitive_routes_when_configured() {
    let _lock = env_lock();
    std::env::set_var(INTERNAL_SERVICE_TOKEN_ALLOWLIST_ENV, "secret-one,secret-two");
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping internal_service_token_blocks_sensitive_routes_when_configured.");
            return;
        }
    };

    let app = register_routes(pool, grpc_none(), None);
    let mut svc = app.into_service();
    let peer = std::net::SocketAddr::from(([127, 0, 0, 1], 9));

    let mut blocked_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    blocked_req.extensions_mut().insert(ConnectInfo(peer));
    let blocked = svc.ready().await.unwrap().call(blocked_req).await.unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);

    let mut ok_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-internal-service-token", "secret-one")
        .body(Body::from(
            json!({ "email": "x", "password": "y" }).to_string(),
        ))
        .unwrap();
    ok_req.extensions_mut().insert(ConnectInfo(peer));
    let ok = svc.ready().await.unwrap().call(ok_req).await.unwrap();
    assert_ne!(ok.status(), StatusCode::FORBIDDEN);

    std::env::remove_var(INTERNAL_SERVICE_TOKEN_ALLOWLIST_ENV);
    std::env::remove_var(JWT_SECRET_ENV);
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
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
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
        hdr_empty(),
        test_connect_info(),
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
        hdr_empty(),
        test_connect_info(),
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

#[tokio::test]
async fn revoke_api_key_unknown_returns_bad_request() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");
    std::env::set_var(API_KEY_HASH_SECRET_ENV, "integration_test_api_key_hash");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping revoke_api_key_unknown_returns_bad_request.");
            return;
        }
    };

    let email = format!("revunk+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };
    let reg = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");
    let RegisterBusinessResponse {
        environments,
        ..
    } = reg.0;
    let sandbox_id = environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox")
        .id;

    let login_sandbox = login(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(LoginRequest {
            email: email.clone(),
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login");
    let access = login_sandbox.0.access_token.clone();

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
        .expect("ctx");

    let err = revoke_api_key(
        State(state),
        hdr_empty(),
        test_connect_info(),
        ctx,
        Path(Uuid::new_v4()),
    )
    .await;
    assert!(matches!(err, Err(AppError::BadRequest(_))));

    std::env::remove_var(JWT_SECRET_ENV);
    std::env::remove_var(API_KEY_HASH_SECRET_ENV);
}

#[tokio::test]
async fn create_api_key_forbidden_for_non_admin_member() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");
    std::env::set_var(API_KEY_HASH_SECRET_ENV, "integration_test_api_key_hash");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping create_api_key_forbidden_for_non_admin_member.");
            return;
        }
    };

    let email = format!("memberkey+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: None,
    };
    let reg = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");
    let RegisterBusinessResponse {
        business_id,
        environments,
        ..
    } = reg.0;
    let sandbox_id = environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox")
        .id;

    let email_norm = email.trim().to_lowercase();
    let admin_hash: String = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE email = $1 AND status = 'active' LIMIT 1",
    )
    .bind(&email_norm)
    .fetch_one(&pool)
    .await
    .expect("admin hash");

    let member_id = Uuid::new_v4();
    let member_email = format!("memberuser+{}@example.com", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO users (id, business_id, environment_id, first_name, last_name, email, password_hash, role, status, created_at, updated_at) VALUES ($1, $2, $3, 'Mem', 'Ber', $4, $5, 'member', 'active', NOW(), NOW())",
    )
    .bind(member_id)
    .bind(business_id)
    .bind(sandbox_id)
    .bind(&member_email)
    .bind(&admin_hash)
    .execute(&pool)
    .await
    .expect("insert member");

    let member_login = login(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(LoginRequest {
            email: member_email,
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("member login");
    let mut parts = empty_request_parts();
    parts.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", member_login.0.access_token).parse().unwrap(),
    );
    parts
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    let ctx_member = AuthContext::from_request_parts(&mut parts, &state)
        .await
        .expect("member ctx");

    let denied = create_api_key(
        State(state),
        hdr_empty(),
        test_connect_info(),
        ctx_member,
        Json(CreateApiKeyRequest {
            environment_id: None,
        }),
    )
    .await;
    assert!(matches!(denied, Err(AppError::Forbidden)));

    std::env::remove_var(JWT_SECRET_ENV);
    std::env::remove_var(API_KEY_HASH_SECRET_ENV);
}

#[tokio::test]
async fn password_reset_request_sends_resend_when_configured() {
    let _lock = env_lock();

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping password_reset_request_sends_resend_when_configured.");
            return;
        }
    };

    let server = MockServer::start_async().await;
    let emails_mock = server
        .mock_async(|when, then| {
            when.method(POST).path("/emails");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"id":"email-pr"}"#);
        })
        .await;

    let database_url = std::env::var(DATABASE_URL_ENV)
        .expect("DATABASE_URL must be set when test_pool() succeeds");
    let config = Config {
        database_url,
        server_addr: "127.0.0.1:0".to_string(),
        grpc_port: 50051,
        accounts_grpc_url: "http://localhost:50052".to_string(),
        audit_grpc_url: "http://127.0.0.1:1".to_string(),
        sentry_dsn: None,
        environment: "test".to_string(),
        resend_api_key: Some("test-key-pr".to_string()),
        resend_from_email: "noreply@rails.co.za".to_string(),
        resend_from_name: "Rails".to_string(),
        resend_base_url: server.base_url(),
        resend_beta_notification_email: "beta@rails.co.za".to_string(),
        frontend_base_url: "http://localhost:5173".to_string(),
    };
    let email_service = EmailService::new(&config);

    let email = format!("prsend+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: Some(email_service),
    };
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");

    let _ = request_password_reset(
        State(state),
        hdr_empty(),
        test_connect_info(),
        Json(RequestPasswordResetRequest {
            email: email.clone(),
        }),
    )
    .await
    .expect("request reset");

    emails_mock.assert_async().await;
}

#[tokio::test]
async fn password_reset_request_logs_when_resend_returns_error() {
    let _lock = env_lock();

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping password_reset_request_logs_when_resend_returns_error.");
            return;
        }
    };

    let server = MockServer::start_async().await;
    let emails_mock = server
        .mock_async(|when, then| {
            when.method(POST).path("/emails");
            then.status(502).body("bad gateway");
        })
        .await;

    let database_url = std::env::var(DATABASE_URL_ENV)
        .expect("DATABASE_URL must be set when test_pool() succeeds");
    let config = Config {
        database_url,
        server_addr: "127.0.0.1:0".to_string(),
        grpc_port: 50051,
        accounts_grpc_url: "http://localhost:50052".to_string(),
        audit_grpc_url: "http://127.0.0.1:1".to_string(),
        sentry_dsn: None,
        environment: "test".to_string(),
        resend_api_key: Some("test-key-pr-err".to_string()),
        resend_from_email: "noreply@rails.co.za".to_string(),
        resend_from_name: "Rails".to_string(),
        resend_base_url: server.base_url(),
        resend_beta_notification_email: "beta@rails.co.za".to_string(),
        frontend_base_url: "http://localhost:5173".to_string(),
    };
    let email_service = EmailService::new(&config);

    let email = format!("prerr+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc: grpc_none(),
        email: Some(email_service),
    };
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");

    let _ = request_password_reset(
        State(state),
        hdr_empty(),
        test_connect_info(),
        Json(RequestPasswordResetRequest {
            email: email.clone(),
        }),
    )
    .await
    .expect("request still returns success when email fails");

    emails_mock.assert_async().await;
}

#[tokio::test]
async fn login_refresh_revoke_paths_with_failing_audit_grpc() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping login_refresh_revoke_paths_with_failing_audit_grpc.");
            return;
        }
    };

    let (grpc, join, shutdown_tx) = grpc_clients_with_failing_audit().await;

    let email = format!("failaudit+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc,
        email: None,
    };

    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");

    let ok = login(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(LoginRequest {
            email: email.clone(),
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login ok despite audit failure");
    assert!(!ok.0.access_token.is_empty());

    let bad_login = login(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(LoginRequest {
            email: email.clone(),
            password: "wrong-password".into(),
            environment_id: None,
        }),
    )
    .await;
    assert!(bad_login.is_err());

    let _ = request_password_reset(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RequestPasswordResetRequest {
            email: "ghost-user-not-real@example.com".into(),
        }),
    )
    .await
    .expect("reset request for missing user");

    let refreshed = refresh_token(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RefreshTokenRequest {
            refresh_token: ok.0.refresh_token.clone(),
        }),
    )
    .await
    .expect("refresh ok");
    assert!(!refreshed.0.access_token.is_empty());

    let bad_revoke = revoke_token(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RevokeTokenRequest {
            refresh_token: "not-a-valid-refresh-token".into(),
        }),
    )
    .await;
    assert!(bad_revoke.is_err());

    let _ = revoke_token(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(RevokeTokenRequest {
            refresh_token: refreshed.0.refresh_token.clone(),
        }),
    )
    .await
    .expect("revoke ok");

    let _ = shutdown_tx.send(());
    let _ = join.await;

    std::env::remove_var(JWT_SECRET_ENV);
}

#[tokio::test]
async fn login_falls_back_to_sandbox_when_requested_environment_id_unknown() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping login_falls_back_to_sandbox_when_requested_environment_id_unknown.");
            return;
        }
    };

    let email = format!("envfallback+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool,
        grpc: grpc_none(),
        email: None,
    };
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");

    let ok = login(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(LoginRequest {
            email: email.clone(),
            password: "password123!".into(),
            environment_id: Some(Uuid::new_v4()),
        }),
    )
    .await
    .expect("login with bogus env id still succeeds");
    let sandbox_type = ok
        .0
        .environments
        .iter()
        .find(|e| e.id == ok.0.selected_environment_id)
        .map(|e| e.r#type.as_str())
        .expect("selected env");
    assert_eq!(sandbox_type, "sandbox");

    std::env::remove_var(JWT_SECRET_ENV);
}

#[tokio::test]
async fn jwt_auth_rejects_environment_not_in_business() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping jwt_auth_rejects_environment_not_in_business.");
            return;
        }
    };

    let email = format!("jwtbadenv+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool,
        grpc: grpc_none(),
        email: None,
    };
    let reg = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");
    let sandbox_id = reg
        .0
        .environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox")
        .id;

    let login_ok = login(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(LoginRequest {
            email,
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login");

    let mut parts = empty_request_parts();
    parts.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", login_ok.0.access_token).parse().unwrap(),
    );
    parts
        .headers
        .insert("x-environment-id", Uuid::new_v4().to_string().parse().unwrap());
    let err = AuthContext::from_request_parts(&mut parts, &state).await;
    assert!(matches!(err, Err(AppError::Forbidden)));

    let mut parts_ok = empty_request_parts();
    parts_ok.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", login_ok.0.access_token).parse().unwrap(),
    );
    parts_ok
        .headers
        .insert("x-environment-id", sandbox_id.to_string().parse().unwrap());
    AuthContext::from_request_parts(&mut parts_ok, &state)
        .await
        .expect("valid env still works");

    std::env::remove_var(JWT_SECRET_ENV);
}

#[tokio::test]
async fn create_api_key_admin_success_with_failing_audit_grpc() {
    let _lock = env_lock();
    std::env::set_var(JWT_SECRET_ENV, "integration_test_jwt_secret");
    std::env::set_var(API_KEY_HASH_SECRET_ENV, "integration_test_api_key_hash");

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping create_api_key_admin_success_with_failing_audit_grpc.");
            return;
        }
    };

    let (grpc, join, shutdown_tx) = grpc_clients_with_failing_audit().await;

    let email = format!("apikeyok+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc,
        email: None,
    };

    let reg = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
    .await
    .expect("register");
    let sandbox_id = reg
        .0
        .environments
        .iter()
        .find(|e| e.r#type == "sandbox")
        .expect("sandbox")
        .id;

    let login_sandbox = login(
        State(state.clone()),
        HeaderMap::new(),
        test_connect_info(),
        Json(LoginRequest {
            email: email.clone(),
            password: "password123!".into(),
            environment_id: None,
        }),
    )
    .await
    .expect("login");
    let access = login_sandbox.0.access_token.clone();

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
        .expect("ctx");

    let key_resp = create_api_key(
        State(state),
        HeaderMap::new(),
        test_connect_info(),
        ctx,
        Json(CreateApiKeyRequest {
            environment_id: None,
        }),
    )
    .await
    .expect("create api key");
    assert_eq!(key_resp.0.status, "active");
    assert!(!key_resp.0.key.is_empty());

    let _ = shutdown_tx.send(());
    let _ = join.await;

    std::env::remove_var(JWT_SECRET_ENV);
    std::env::remove_var(API_KEY_HASH_SECRET_ENV);
}

#[tokio::test]
async fn reset_password_success_with_failing_audit_grpc() {
    let _lock = env_lock();

    let pool = match test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("DATABASE_URL not set; skipping reset_password_success_with_failing_audit_grpc.");
            return;
        }
    };

    let (grpc, join, shutdown_tx) = grpc_clients_with_failing_audit().await;

    let email = format!("seedfail+{}@example.com", Uuid::new_v4());
    let state = AppState {
        db: pool.clone(),
        grpc,
        email: None,
    };
    let _ = register_business(
        State(state.clone()),
        hdr_empty(),
        test_connect_info(),
        Json(register_payload(&email)),
    )
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
        State(state),
        hdr_empty(),
        test_connect_info(),
        Json(ResetPasswordRequest {
            token: raw_token.into(),
            new_password: "newpassword1!".into(),
        }),
    )
    .await
    .expect("reset password");
    assert!(done.0.message.to_lowercase().contains("success"));

    let _ = shutdown_tx.send(());
    let _ = join.await;
}
