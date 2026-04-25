//! E2E: `emit_users_mutation` → real audit gRPC → Postgres `audit_events` row (RAI-14).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use audit_service::grpc_server::AuditGrpcService;
use axum::http::HeaderMap;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use uuid::Uuid;

use users_service::audit_emit;
use users_service::grpc::audit_proto::ActorType;
use users_service::grpc::{audit_proto::audit_service_client::AuditServiceClient, GrpcClients};
use tonic::transport::Endpoint;

async fn start_audit_stack() -> (
    sqlx::PgPool,
    GrpcClients,
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
        .max_connections(15)
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
    let audit_client = AuditServiceClient::new(ch);
    let grpc = GrpcClients::new(None, Some(audit_client));

    (pool, grpc, join, shutdown_tx)
}

// Multi-thread: tonic server runs on spawned tasks; a single-thread test runtime can deadlock
// waiting on the client while the server never gets polled → sqlx pool acquire timeouts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker (testcontainers); CI runs: cargo test --locked -- --include-ignored"]
async fn users_emit_persists_audit_row() {
    let (pool, grpc, join, shutdown_tx) = start_audit_stack().await;

    let correlation_id = format!("e2e-users-{}", Uuid::new_v4());
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-correlation-id",
        correlation_id.parse().expect("header value"),
    );
    headers.insert("x-environment", "sandbox".parse().unwrap());

    let peer = SocketAddr::from(([127, 0, 0, 1], 42_042));
    let target_user = Uuid::new_v4();

    audit_emit::emit_users_mutation(
        &grpc,
        &headers,
        &peer,
        "POST",
        "/api/v1/auth/login",
        "users.auth.login",
        Uuid::nil(),
        ActorType::Anonymous,
        "",
        vec![],
        "user",
        target_user,
        401,
        None,
        HashMap::new(),
    )
    .await;

    let row = sqlx::query(
        "SELECT source_service, action, correlation_id FROM audit_events WHERE correlation_id = $1",
    )
    .bind(&correlation_id)
    .fetch_one(&pool)
    .await
    .expect("audit row");
    let source: String = row.try_get("source_service").unwrap();
    let action: String = row.try_get("action").unwrap();
    let cid: String = row.try_get("correlation_id").unwrap();
    assert_eq!(source, "users");
    assert_eq!(action, "users.auth.login");
    assert_eq!(cid, correlation_id);

    let _ = shutdown_tx.send(());
    let _ = join.await;
}
