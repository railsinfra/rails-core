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
use accounts_api::users_grpc::users_proto::{ValidateApiKeyRequest, ValidateApiKeyResponse};

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

#[tonic::async_trait]
impl UsersService for UsersOk {
    async fn validate_api_key(
        &self,
        _req: GrpcRequest<ValidateApiKeyRequest>,
    ) -> Result<GrpcResponse<ValidateApiKeyResponse>, Status> {
        Ok(GrpcResponse::new(ValidateApiKeyResponse {
            business_id: Uuid::nil().to_string(),
            environment_id: Uuid::nil().to_string(),
            admin_user_id: Uuid::nil().to_string(),
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
