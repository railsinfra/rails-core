//! E2E: Testcontainers Postgres + tonic `AuditService` + row persisted in `audit_events`.

use std::path::Path;
use std::time::Duration;

use audit_service::grpc_server::AuditGrpcService;
use audit_service::proto::proto::audit_service_client::AuditServiceClient;
use audit_service::proto::proto::{
    Actor, AppendAuditEventRequest, AuditEvent, Outcome, RequestContext, Target,
};
use audit_service::proto::proto::ActorType;
use chrono::Utc;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::Request;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires Docker (testcontainers); CI runs: cargo test --locked -- --include-ignored"]
async fn append_audit_event_persists_row() {
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
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");

    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let migrator = Migrator::new(migrations).await.expect("migrator");
    migrator.run(&pool).await.expect("migrate");

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
    tokio::time::sleep(Duration::from_millis(80)).await;

    let mut client = AuditServiceClient::connect(format!("http://{addr}"))
        .await
        .expect("connect audit grpc");

    let org = Uuid::new_v4();
    let correlation_id = format!("e2e-audit-core-{}", Uuid::new_v4());
    let ev = AuditEvent {
        occurred_at: Utc::now().to_rfc3339(),
        schema_version: 1,
        source_service: "ledger".into(),
        organization_id: org.to_string(),
        environment: "sandbox".into(),
        actor: Some(Actor {
            r#type: ActorType::InternalService as i32,
            id: "ledger".into(),
            roles: vec![],
        }),
        action: "ledger.transaction.post".into(),
        target: Some(Target {
            r#type: "ledger_transaction".into(),
            id: Uuid::new_v4().to_string(),
        }),
        outcome: Outcome::Success as i32,
        request: Some(RequestContext {
            id: Uuid::new_v4().to_string(),
            method: "POST".into(),
            path: "/grpc/LedgerService/PostTransaction".into(),
            ip: "127.0.0.1".into(),
            user_agent: "e2e".into(),
        }),
        correlation_id: correlation_id.clone(),
        reason: None,
        metadata: std::collections::HashMap::default(),
    };

    let mut req = Request::new(AppendAuditEventRequest { event: Some(ev) });
    req.set_timeout(Duration::from_secs(5));
    let res = client.append_audit_event(req).await.expect("append");
    let returned_id = Uuid::parse_str(&res.into_inner().audit_event_id).expect("uuid");
    assert_ne!(returned_id, Uuid::nil());

    let row = sqlx::query(
        "SELECT id, action, source_service FROM audit_events WHERE correlation_id = $1",
    )
    .bind(&correlation_id)
    .fetch_one(&pool)
    .await
    .expect("row in audit_events");
    let db_id: Uuid = row.try_get("id").expect("id");
    let action: String = row.try_get("action").expect("action");
    let source: String = row.try_get("source_service").expect("source_service");
    assert_eq!(db_id, returned_id);
    assert_eq!(action, "ledger.transaction.post");
    assert_eq!(source, "ledger");

    let _ = shutdown_tx.send(());
    let _ = join.await;
}
