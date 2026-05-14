use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use accounts_api::grpc::audit_channel;
use accounts_api::grpc::audit_proto::audit_service_server::{AuditService, AuditServiceServer};
use accounts_api::grpc::audit_proto::{AppendAuditEventRequest, AppendAuditEventResponse};
use accounts_api::grpc::ledger_proto::ledger_service_server::{LedgerService, LedgerServiceServer};
use accounts_api::grpc::ledger_proto::{
    GetAccountBalanceRequest, GetAccountBalanceResponse, GetAccountBalancesRequest,
    GetAccountBalancesResponse, PostTransactionRequest, PostTransactionResponse,
};
use accounts_api::ledger_grpc::LedgerGrpc;
use accounts_api::routes::create_router;
use accounts_api::users_grpc::users_proto::users_service_server::{
    UsersService, UsersServiceServer,
};
use accounts_api::users_grpc::users_proto::{
    ResolveAccountUserForApiKeyRequest, ResolveAccountUserForApiKeyResponse, ValidateApiKeyRequest,
    ValidateApiKeyResponse,
};

use axum::serve;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request as GrpcRequest, Response as GrpcResponse, Status};
use uuid::Uuid;

async fn migrated_accounts_pool() -> (testcontainers::ContainerAsync<Postgres>, PgPool) {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres testcontainer");
    let host = container.get_host().await.expect("container host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("container port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to test postgres");
    sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#)
        .execute(&pool)
        .await
        .expect("create pgcrypto extension for gen_random_uuid");
    sqlx::migrate!("./migrations_accounts")
        .run(&pool)
        .await
        .expect("run migrations_accounts");
    (container, pool)
}

#[derive(Clone, Default)]
struct UsersOk;

const MOCK_BUSINESS_ID: &str = "11111111-1111-1111-1111-111111111111";
const MOCK_ENVIRONMENT_ID: &str = "22222222-2222-2222-2222-222222222222";
const MOCK_ADMIN_USER_ID: &str = "33333333-3333-3333-3333-333333333333";
const MOCK_ACCOUNT_USER_ID: &str = "44444444-4444-4444-4444-444444444444";

#[tonic::async_trait]
impl UsersService for UsersOk {
    async fn validate_api_key(
        &self,
        _req: GrpcRequest<ValidateApiKeyRequest>,
    ) -> Result<GrpcResponse<ValidateApiKeyResponse>, Status> {
        Ok(GrpcResponse::new(ValidateApiKeyResponse {
            business_id: MOCK_BUSINESS_ID.to_string(),
            environment_id: MOCK_ENVIRONMENT_ID.to_string(),
            admin_user_id: MOCK_ADMIN_USER_ID.to_string(),
        }))
    }

    async fn resolve_account_user_for_api_key(
        &self,
        req: GrpcRequest<ResolveAccountUserForApiKeyRequest>,
    ) -> Result<GrpcResponse<ResolveAccountUserForApiKeyResponse>, Status> {
        let req = req.into_inner();
        if req.api_key != "test-key" {
            return Err(Status::unauthenticated("invalid api key"));
        }
        if req.email != "new-holder@example.com"
            || req.first_name != "New"
            || req.last_name != "Holder"
        {
            return Err(Status::not_found(
                "No active user matches the supplied account owner details",
            ));
        }
        Ok(GrpcResponse::new(ResolveAccountUserForApiKeyResponse {
            business_id: MOCK_BUSINESS_ID.to_string(),
            environment_id: MOCK_ENVIRONMENT_ID.to_string(),
            user_id: MOCK_ACCOUNT_USER_ID.to_string(),
        }))
    }
}

async fn spawn_users_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(UsersServiceServer::new(UsersOk::default()))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    format!("http://{}", addr)
}

#[derive(Clone)]
struct CountingAudit {
    hits: Arc<AtomicUsize>,
}

#[tonic::async_trait]
impl AuditService for CountingAudit {
    async fn append_audit_event(
        &self,
        _request: GrpcRequest<AppendAuditEventRequest>,
    ) -> Result<GrpcResponse<AppendAuditEventResponse>, Status> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(GrpcResponse::new(AppendAuditEventResponse {
            audit_event_id: Uuid::new_v4().to_string(),
        }))
    }
}

async fn spawn_audit_server(hits: Arc<AtomicUsize>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    let svc = CountingAudit { hits };
    tokio::spawn(async move {
        Server::builder()
            .add_service(AuditServiceServer::new(svc))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    format!("http://{}", addr)
}

#[derive(Clone, Default)]
struct LedgerOk;

#[tonic::async_trait]
impl LedgerService for LedgerOk {
    async fn post_transaction(
        &self,
        _req: GrpcRequest<PostTransactionRequest>,
    ) -> Result<GrpcResponse<PostTransactionResponse>, Status> {
        Ok(GrpcResponse::new(PostTransactionResponse {
            status: "posted".into(),
            ledger_transaction_id: String::default(),
            failure_reason: String::default(),
        }))
    }

    async fn get_account_balance(
        &self,
        req: GrpcRequest<GetAccountBalanceRequest>,
    ) -> Result<GrpcResponse<GetAccountBalanceResponse>, Status> {
        let r = req.into_inner();
        // Liability balances are negative in ledger; keep plenty of funds for withdraw pre-check.
        let bal = if r.external_account_id.contains("low_funds") {
            "-1000".to_string()
        } else {
            "-100000000".to_string()
        };
        Ok(GrpcResponse::new(GetAccountBalanceResponse {
            balance: bal,
            currency: r.currency,
        }))
    }

    async fn get_account_balances(
        &self,
        req: GrpcRequest<GetAccountBalancesRequest>,
    ) -> Result<GrpcResponse<GetAccountBalancesResponse>, Status> {
        let r = req.into_inner();
        Ok(GrpcResponse::new(GetAccountBalancesResponse {
            from_balance: "-200000".to_string(),
            to_balance: "-300000".to_string(),
            currency: r.currency,
        }))
    }
}

async fn spawn_ledger_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(LedgerServiceServer::new(LedgerOk::default()))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    format!("http://{}", addr)
}

async fn insert_active_account(
    pool: &PgPool,
    org: Uuid,
    env: &str,
    user_id: Uuid,
    account_number: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, account_number, account_type, user_id, currency, status,
            organization_id, environment
        )
        VALUES ($1, $2, 'checking', $3, 'USD', 'active', $4, $5)
        "#,
    )
    .bind(id)
    .bind(account_number)
    .bind(user_id)
    .bind(org)
    .bind(env)
    .execute(pool)
    .await
    .expect("insert account");
    id
}

async fn http_post_json(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    idem: &str,
    body: serde_json::Value,
) -> u16 {
    let url = format!("{base_url}{path}");
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-environment", "sandbox")
        .header("Idempotency-Key", idem)
        .json(&body)
        .send()
        .await
        .expect("http request");
    let status = resp.status().as_u16();
    if status != 200 {
        let text = resp.text().await.unwrap_or_default();
        panic!("unexpected status={status} url={path} body={text}");
    }
    status
}

async fn http_post_json_expect_status(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    idem: &str,
    body: serde_json::Value,
    expected_status: u16,
) -> u16 {
    let url = format!("{base_url}{path}");
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-environment", "sandbox")
        .header("Idempotency-Key", idem)
        .json(&body)
        .send()
        .await
        .expect("http request");
    let status = resp.status().as_u16();
    if status != expected_status {
        let text = resp.text().await.unwrap_or_default();
        panic!("unexpected status={status} expected={expected_status} url={path} body={text}");
    }
    status
}

#[tokio::test]
async fn account_creation_requires_existing_user_and_stores_user_id() {
    let (_c, pool) = migrated_accounts_pool().await;

    let users_url = spawn_users_server().await;
    let ledger_url = spawn_ledger_server().await;
    let audit_hits = Arc::new(AtomicUsize::new(0));
    let audit_url = spawn_audit_server(audit_hits).await;

    let users_grpc = accounts_api::users_grpc::UsersGrpc::connect_lazy(&users_url).unwrap();
    let ledger_grpc = LedgerGrpc::new(ledger_url);
    let audit_client = audit_channel(&audit_url);

    let app = create_router(pool.clone(), ledger_grpc, users_grpc, audit_client);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/accounts"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("x-environment", "sandbox")
        .json(&json!({
            "account_type": "checking",
            "currency": "USD",
            "email": "new-holder@example.com",
            "first_name": "New",
            "last_name": "Holder"
        }))
        .send()
        .await
        .expect("create holder account");

    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.expect("json body");
    assert_eq!(status, 201, "unexpected body={body}");
    assert!(
        body.get("holder_id").is_some_and(|v| v.is_null()),
        "holder_id should not be present for existing-user accounts: {body}"
    );
    assert_eq!(
        body.get("organization_id").and_then(|v| v.as_str()),
        Some(MOCK_BUSINESS_ID)
    );
    assert_eq!(
        body.get("user_id").and_then(|v| v.as_str()),
        Some(MOCK_ACCOUNT_USER_ID)
    );
    assert!(
        body.get("admin_user_id").is_some_and(|v| v.is_null()),
        "{body}"
    );
    let holder_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_holders")
        .fetch_one(&pool)
        .await
        .expect("count account holders");
    assert_eq!(holder_count, 0);

    let duplicate = client
        .post(format!("http://{addr}/api/v1/accounts"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("x-environment", "sandbox")
        .json(&json!({
            "account_type": "checking",
            "currency": "USD",
            "email": "new-holder@example.com",
            "first_name": "New",
            "last_name": "Holder"
        }))
        .send()
        .await
        .expect("duplicate account");
    assert_eq!(duplicate.status().as_u16(), 400);
    let duplicate_body: serde_json::Value = duplicate.json().await.expect("duplicate json");
    assert_eq!(duplicate_body["status"], 400);
    assert!(
        duplicate_body["message"]
            .as_str()
            .is_some_and(|message| message.contains("already has a checking account")),
        "{duplicate_body}"
    );

    server.abort();
}

#[tokio::test]
async fn account_creation_rejects_non_matching_user_and_legacy_owner_payloads() {
    let (_c, pool) = migrated_accounts_pool().await;

    let users_url = spawn_users_server().await;
    let ledger_url = spawn_ledger_server().await;
    let audit_hits = Arc::new(AtomicUsize::new(0));
    let audit_url = spawn_audit_server(audit_hits).await;

    let users_grpc = accounts_api::users_grpc::UsersGrpc::connect_lazy(&users_url).unwrap();
    let ledger_grpc = LedgerGrpc::new(ledger_url);
    let audit_client = audit_channel(&audit_url);

    let app = create_router(pool, ledger_grpc, users_grpc, audit_client);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let client = reqwest::Client::new();

    let missing_api_key = client
        .post(format!("http://{addr}/api/v1/accounts"))
        .header("content-type", "application/json")
        .header("x-environment", "sandbox")
        .json(&json!({
            "account_type": "checking",
            "currency": "USD",
            "email": "new-holder@example.com",
            "first_name": "New",
            "last_name": "Holder"
        }))
        .send()
        .await
        .expect("missing api key");
    assert_eq!(missing_api_key.status().as_u16(), 400);

    for (field, value, expected_message) in [
        ("email", "", "email is required"),
        ("first_name", " ", "first_name is required"),
        ("last_name", "\t", "last_name is required"),
    ] {
        let mut payload = json!({
            "account_type": "checking",
            "currency": "USD",
            "email": "new-holder@example.com",
            "first_name": "New",
            "last_name": "Holder"
        });
        payload[field] = json!(value);
        let resp = client
            .post(format!("http://{addr}/api/v1/accounts"))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .header("x-environment", "sandbox")
            .json(&payload)
            .send()
            .await
            .expect("blank account owner field");
        assert_eq!(resp.status().as_u16(), 400);
        let body: serde_json::Value = resp.json().await.expect("blank field json");
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|message| message.contains(expected_message)),
            "{body}"
        );
    }

    let mismatch = client
        .post(format!("http://{addr}/api/v1/accounts"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("x-environment", "sandbox")
        .header("x-correlation-id", "cid-account-mismatch")
        .json(&json!({
            "account_type": "checking",
            "currency": "USD",
            "email": "new-holder@example.com",
            "first_name": "Wrong",
            "last_name": "Holder"
        }))
        .send()
        .await
        .expect("create mismatch account");
    assert_eq!(mismatch.status().as_u16(), 400);
    let mismatch_body: serde_json::Value = mismatch.json().await.expect("mismatch json");
    assert_eq!(mismatch_body["status"], 400);
    assert_eq!(mismatch_body["correlationId"], "cid-account-mismatch");
    assert!(mismatch_body
        .get("timestamp")
        .and_then(|v| v.as_str())
        .is_some());

    let legacy = client
        .post(format!("http://{addr}/api/v1/accounts"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("x-environment", "sandbox")
        .json(&json!({
            "account_type": "checking",
            "currency": "USD",
            "user_id": MOCK_ACCOUNT_USER_ID
        }))
        .send()
        .await
        .expect("legacy create account");
    assert!(legacy.status().is_client_error());
    let legacy_body: serde_json::Value = legacy.json().await.expect("legacy json");
    assert!(legacy_body.get("status").and_then(|v| v.as_u64()).is_some());
    assert!(legacy_body
        .get("message")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(legacy_body
        .get("correlationId")
        .and_then(|v| v.as_str())
        .is_some());
    assert!(legacy_body
        .get("timestamp")
        .and_then(|v| v.as_str())
        .is_some());

    server.abort();
}

#[tokio::test]
async fn deposit_withdraw_transfer_exercise_ledger_and_background_audit() {
    let (_c, pool) = migrated_accounts_pool().await;
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();

    let a1 = insert_active_account(&pool, org, "sandbox", user, "1000000000000001").await;
    let a2 = insert_active_account(&pool, org, "sandbox", user, "1000000000000002").await;

    let users_url = spawn_users_server().await;
    let ledger_url = spawn_ledger_server().await;
    let audit_hits = Arc::new(AtomicUsize::new(0));
    let audit_url = spawn_audit_server(audit_hits.clone()).await;

    let users_grpc = accounts_api::users_grpc::UsersGrpc::connect_lazy(&users_url).unwrap();
    let ledger_grpc = LedgerGrpc::new(ledger_url);
    let audit_client = audit_channel(&audit_url);

    let app = create_router(pool, ledger_grpc, users_grpc, audit_client);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let base_url = format!("http://{addr}");
    let client = reqwest::Client::new();

    assert_eq!(
        http_post_json(
            &client,
            &base_url,
            &format!("/api/v1/accounts/{a1}/deposit"),
            "idem-deposit-1",
            json!({"amount": 1000}),
        )
        .await,
        200
    );
    assert_eq!(
        http_post_json(
            &client,
            &base_url,
            &format!("/api/v1/accounts/{a1}/withdraw"),
            "idem-withdraw-1",
            json!({"amount": 100}),
        )
        .await,
        200
    );
    assert_eq!(
        http_post_json(
            &client,
            &base_url,
            &format!("/api/v1/accounts/{a1}/transfer"),
            "idem-transfer-1",
            json!({"to_account_id": a2, "amount": 50}),
        )
        .await,
        200
    );

    server.abort();

    // Background audit emits should complete quickly in tests.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        audit_hits.load(Ordering::SeqCst) >= 3,
        "expected audit append calls for deposit+withdraw+transfer"
    );
}

#[derive(Clone, Default)]
struct LedgerFailPost;

#[tonic::async_trait]
impl LedgerService for LedgerFailPost {
    async fn post_transaction(
        &self,
        _req: GrpcRequest<PostTransactionRequest>,
    ) -> Result<GrpcResponse<PostTransactionResponse>, Status> {
        Err(Status::internal("forced ledger post failure"))
    }

    async fn get_account_balance(
        &self,
        req: GrpcRequest<GetAccountBalanceRequest>,
    ) -> Result<GrpcResponse<GetAccountBalanceResponse>, Status> {
        let r = req.into_inner();
        Ok(GrpcResponse::new(GetAccountBalanceResponse {
            balance: "-100000000".to_string(),
            currency: r.currency,
        }))
    }

    async fn get_account_balances(
        &self,
        req: GrpcRequest<GetAccountBalancesRequest>,
    ) -> Result<GrpcResponse<GetAccountBalancesResponse>, Status> {
        let r = req.into_inner();
        Ok(GrpcResponse::new(GetAccountBalancesResponse {
            from_balance: "-200000".to_string(),
            to_balance: "-300000".to_string(),
            currency: r.currency,
        }))
    }
}

async fn spawn_ledger_failing_post_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(LedgerServiceServer::new(LedgerFailPost::default()))
            .serve_with_incoming(incoming)
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    format!("http://{}", addr)
}

#[tokio::test]
async fn deposit_returns_202_when_ledger_post_is_deferred() {
    let (_c, pool) = migrated_accounts_pool().await;
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    let account_id = insert_active_account(&pool, org, "sandbox", user, "1000000000000010").await;

    let users_url = spawn_users_server().await;
    let ledger_url = spawn_ledger_failing_post_server().await;
    let audit_hits = Arc::new(AtomicUsize::new(0));
    let audit_url = spawn_audit_server(audit_hits).await;

    let users_grpc = accounts_api::users_grpc::UsersGrpc::connect_lazy(&users_url).unwrap();
    let ledger_grpc = LedgerGrpc::new(ledger_url);
    let audit_client = audit_channel(&audit_url);
    let app = create_router(pool, ledger_grpc, users_grpc, audit_client);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let base_url = format!("http://{addr}");
    let client = reqwest::Client::new();
    let status = http_post_json_expect_status(
        &client,
        &base_url,
        &format!("/api/v1/accounts/{account_id}/deposit"),
        "idem-deposit-202",
        json!({"amount": 1000}),
        202,
    )
    .await;
    assert_eq!(status, 202);

    server.abort();
}
