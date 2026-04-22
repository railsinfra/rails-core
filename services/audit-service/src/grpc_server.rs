//! gRPC `AppendAuditEvent` server.

use chrono::{DateTime, Utc};
use sentry::TransactionContext;
use serde_json::{Map, Value};
use sqlx::PgPool;
use tonic::{Request, Response, Status};
use tracing::Instrument;
use uuid::Uuid;

use crate::db::{insert_audit_event, AuditInsert};
use crate::proto::proto::audit_service_server::{AuditService, AuditServiceServer};
use crate::proto::proto::{
    ActorType, AppendAuditEventRequest, AppendAuditEventResponse, AuditEvent, Outcome,
};
use crate::validate::validate_audit_event;

#[derive(Clone)]
pub struct AuditGrpcService {
    pool: PgPool,
}

impl AuditGrpcService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn into_server(self) -> AuditServiceServer<Self> {
        AuditServiceServer::new(self)
    }
}

fn actor_type_str(t: ActorType) -> &'static str {
    match t {
        ActorType::User => "user",
        ActorType::ApiKey => "api_key",
        ActorType::InternalService => "internal_service",
        ActorType::Anonymous => "anonymous",
        ActorType::Unspecified => "unspecified",
    }
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Success => "success",
        Outcome::ClientError => "client_error",
        Outcome::ServerError => "server_error",
        Outcome::Unspecified => "unspecified",
    }
}

fn event_to_insert(event: &AuditEvent) -> Result<AuditInsert, Status> {
    let occurred_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&event.occurred_at)
        .map_err(|_| Status::invalid_argument("occurred_at"))?
        .with_timezone(&Utc);

    let org = Uuid::parse_str(event.organization_id.trim())
        .map_err(|_| Status::invalid_argument("organization_id"))?;

    let actor = event.actor.as_ref().ok_or_else(|| Status::invalid_argument("actor"))?;
    let actor_type = ActorType::try_from(actor.r#type).map_err(|_| Status::invalid_argument("actor.type"))?;

    let target = event.target.as_ref().ok_or_else(|| Status::invalid_argument("target"))?;
    let req = event.request.as_ref().ok_or_else(|| Status::invalid_argument("request"))?;

    let outcome = Outcome::try_from(event.outcome).map_err(|_| Status::invalid_argument("outcome"))?;

    let mut meta_map = Map::new();
    for (k, v) in &event.metadata {
        meta_map.insert(k.clone(), Value::String(v.clone()));
    }

    Ok(AuditInsert {
        occurred_at,
        schema_version: event.schema_version as i16,
        source_service: event.source_service.trim().to_string(),
        organization_id: org,
        environment: event.environment.trim().to_string(),
        actor_type: actor_type_str(actor_type).to_string(),
        actor_id: actor.id.trim().to_string(),
        actor_roles: actor.roles.clone(),
        action: event.action.trim().to_string(),
        target_type: target.r#type.trim().to_string(),
        target_id: target.id.trim().to_string(),
        outcome: outcome_str(outcome).to_string(),
        request_id: req.id.trim().to_string(),
        request_method: req.method.trim().to_string(),
        request_path: req.path.trim().to_string(),
        request_ip: req.ip.clone(),
        request_user_agent: req.user_agent.clone(),
        correlation_id: event.correlation_id.trim().to_string(),
        reason: event.reason.clone().filter(|s| !s.trim().is_empty()),
        metadata: Value::Object(meta_map),
    })
}

#[tonic::async_trait]
impl AuditService for AuditGrpcService {
    async fn append_audit_event(
        &self,
        request: Request<AppendAuditEventRequest>,
    ) -> Result<Response<AppendAuditEventResponse>, Status> {
        let txn = sentry::start_transaction(TransactionContext::new(
            "audit.AppendAuditEvent",
            "grpc.server",
        ));
        let _txn_guard = txn.clone();

        let inner = request.into_inner();
        let event = inner.event.ok_or_else(|| Status::invalid_argument("event required"))?;

        validate_audit_event(&event)?;

        sentry::add_breadcrumb(sentry::Breadcrumb {
            category: Some("audit".into()),
            message: Some("audit.append_request_serialized".into()),
            level: sentry::Level::Info,
            ..Default::default()
        });

        let row = event_to_insert(&event)?;
        let span = txn.start_child("db.write", "audit.insert_audit_event");
        let insert_fut = insert_audit_event(&self.pool, row);
        let id = insert_fut
            .instrument(tracing::info_span!("audit.insert_audit_event"))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        span.finish();

        sentry::add_breadcrumb(sentry::Breadcrumb {
            category: Some("audit".into()),
            message: Some(format!("audit.append_response audit_event_id={id}")),
            level: sentry::Level::Info,
            ..Default::default()
        });

        txn.finish();
        Ok(Response::new(AppendAuditEventResponse {
            audit_event_id: id.to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::proto::{Actor, RequestContext, Target};
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn append_round_trip_against_postgres() {
        let url = match std::env::var("AUDIT_DATABASE_URL") {
            Ok(u) if !u.is_empty() => u,
            _ => {
                eprintln!("skip append_round_trip_against_postgres: AUDIT_DATABASE_URL unset");
                return;
            }
        };

        let pool = PgPool::connect(&url).await.expect("connect");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrate");

        let svc = AuditGrpcService::new(pool.clone());
        let server = AuditGrpcService::into_server(svc);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, rx) = oneshot::channel::<()>();
        let serve = tonic::transport::Server::builder()
            .add_service(server)
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = rx.await;
                },
            );
        let j = tokio::spawn(serve);

        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut client = crate::proto::proto::audit_service_client::AuditServiceClient::connect(format!(
            "http://{addr}"
        ))
        .await
        .unwrap();

        let ev = AuditEvent {
            occurred_at: Utc::now().to_rfc3339(),
            schema_version: 1,
            source_service: "ledger".into(),
            organization_id: Uuid::new_v4().to_string(),
            environment: "sandbox".into(),
            actor: Some(Actor {
                r#type: ActorType::InternalService as i32,
                id: Uuid::new_v4().to_string(),
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
                path: "/grpc/PostTransaction".into(),
                ip: "".into(),
                user_agent: "".into(),
            }),
            correlation_id: "corr-itest".into(),
            reason: None,
            metadata: Default::default(),
        };

        let mut req = Request::new(AppendAuditEventRequest { event: Some(ev) });
        req.set_timeout(Duration::from_secs(2));
        let res = client.append_audit_event(req).await.unwrap();
        let id = Uuid::parse_str(&res.into_inner().audit_event_id).unwrap();
        assert_ne!(id, Uuid::nil());

        let _ = tx.send(());
        let _ = j.await;
    }
}
