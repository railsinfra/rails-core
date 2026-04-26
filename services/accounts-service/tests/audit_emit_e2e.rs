//! E2E: `emit_accounts_mutation` → real audit gRPC → Postgres `audit_events` row (RAI-14).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use accounts_api::audit_emit;
use accounts_api::grpc::audit_proto::audit_service_client::AuditServiceClient;
use accounts_api::grpc::audit_proto::ActorType;
use audit_service::grpc_server::AuditGrpcService;
use axum::http::HeaderMap;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};
use uuid::Uuid;

async fn start_audit_stack() -> (
    ContainerAsync<Postgres>,
    sqlx::PgPool,
    Option<AuditServiceClient<Channel>>,
    tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    tokio::sync::oneshot::Sender<()>,
) {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres (requires Docker for testcontainers)");
    let host = container.get_host().await.expect("host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(90))
        .connect(&url)
        .await
        .expect("connect");

    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../audit-service/migrations");
    let migrator = Migrator::new(migrations).await.expect("migrator");
    migrator.run(&pool).await.expect("migrate audit schema");

    let svc = AuditGrpcService::new(pool.clone());
    let server = AuditGrpcService::into_server(svc);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve = Server::builder()
        .add_service(server)
        .serve_with_incoming_shutdown(
            TcpListenerStream::new(listener),
            async {
                let _ = shutdown_rx.await;
            },
        );
    let join = tokio::spawn(serve);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let ch = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect_lazy();
    let client = AuditServiceClient::new(ch);

    (container, pool, Some(client), join, shutdown_tx)
}

// Multi-thread: see users-service `audit_emit_e2e` — tonic + sqlx need concurrent polling under CI.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker (testcontainers); CI runs: cargo test --locked -- --include-ignored"]
async fn accounts_emit_persists_audit_row() {
    let (_container, pool, audit_client, join, shutdown_tx) = start_audit_stack().await;

    let correlation_id = format!("e2e-accounts-{}", Uuid::new_v4());
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-correlation-id",
        correlation_id.parse().expect("header value"),
    );
    headers.insert("x-environment", "sandbox".parse().unwrap());

    let peer = SocketAddr::from(([127, 0, 0, 1], 42_043));
    let org = Uuid::new_v4();
    let account_id = Uuid::new_v4();

    audit_emit::emit_accounts_mutation(
        &audit_client,
        &headers,
        &peer,
        "POST",
        "/api/v1/accounts",
        "accounts.account.create",
        org,
        ActorType::User,
        &Uuid::new_v4().to_string(),
        vec![],
        "account",
        account_id,
        201,
        None,
        HashMap::new(),
    )
    .await;

    let row = sqlx::query(
        "SELECT source_service, action, organization_id::text FROM audit_events WHERE correlation_id = $1",
    )
    .bind(&correlation_id)
    .fetch_one(&pool)
    .await
    .expect("audit row");
    let source: String = row.try_get("source_service").unwrap();
    let action: String = row.try_get("action").unwrap();
    let org_db: String = row.try_get("organization_id").unwrap();
    assert_eq!(source, "accounts");
    assert_eq!(action, "accounts.account.create");
    assert_eq!(org_db, org.to_string());

    let _ = shutdown_tx.send(());
    let _ = join.await;
}
