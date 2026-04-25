//! Append audit events after users mutations (RAI-14). Failures never change the HTTP outcome.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use axum::http::HeaderMap;
use chrono::Utc;
use sentry::TransactionContext;
use tonic::Request;
use uuid::Uuid;

use crate::error::AppError;
use crate::grpc::audit_proto::audit_service_client::AuditServiceClient;
use crate::grpc::audit_proto::{
    Actor, AppendAuditEventRequest, AuditEvent, Outcome, RequestContext, Target,
};
use crate::grpc::audit_proto::ActorType;
use crate::grpc::GrpcClients;
use tonic::transport::Channel;

pub fn environment_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-environment")
        .or_else(|| headers.get("X-Environment"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn correlation_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn client_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next().map(|x| x.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| addr.ip().to_string())
}

fn user_agent(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

pub fn http_status_for_error(e: &AppError) -> u16 {
    e.status_code()
}

fn outcome_from_status(status: u16) -> i32 {
    if status >= 500 {
        Outcome::ServerError as i32
    } else if status >= 400 {
        Outcome::ClientError as i32
    } else {
        Outcome::Success as i32
    }
}

pub fn truncate_reason(s: &str) -> String {
    let mut out: String = s.chars().take(500).collect();
    if s.chars().count() > 500 {
        out.push('…');
    }
    out
}

/// Deadline for each `AppendAuditEvent` RPC. Remote Postgres (e.g. Neon) often needs >400ms.
fn audit_append_deadline() -> Duration {
    const AUDIT_APPEND_TIMEOUT_MS_ENV: &str = "AUDIT_APPEND_TIMEOUT_MS";
    const DEFAULT_MS: u64 = 5_000;
    const MAX_MS: u64 = 120_000;
    std::env::var(AUDIT_APPEND_TIMEOUT_MS_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&ms| ms > 0 && ms <= MAX_MS)
        .map_or_else(
            || Duration::from_millis(DEFAULT_MS),
            Duration::from_millis,
        )
}

/// Best-effort audit RPC (`AUDIT_APPEND_TIMEOUT_MS`, default 5s). Logs + Sentry on errors; never affects HTTP status.
pub async fn emit_users_mutation(
    grpc: &GrpcClients,
    headers: &HeaderMap,
    peer: &SocketAddr,
    method: &str,
    path: &str,
    action: &'static str,
    organization_id: Uuid,
    actor_type: ActorType,
    actor_id: &str,
    actor_roles: Vec<String>,
    target_type: &str,
    target_id: Uuid,
    http_status: u16,
    reason: Option<String>,
    metadata: HashMap<String, String>,
) {
    let Some(client) = grpc.audit_client.as_ref() else {
        return;
    };

    let txn = sentry::start_transaction(TransactionContext::new(
        "users.audit.emit",
        "audit.emit",
    ));
    sentry::configure_scope(|scope| {
        scope.set_tag("audit.action", action);
    });
    let _guard = txn.clone();

    let outcome = outcome_from_status(http_status);
    let req_id = Uuid::new_v4().to_string();
    let event = AuditEvent {
        occurred_at: Utc::now().to_rfc3339(),
        schema_version: 1,
        source_service: "users".into(),
        organization_id: organization_id.to_string(),
        environment: environment_from_headers(headers),
        actor: Some(Actor {
            r#type: actor_type as i32,
            id: actor_id.to_string(),
            roles: actor_roles,
        }),
        action: action.to_string(),
        target: Some(Target {
            r#type: target_type.to_string(),
            id: target_id.to_string(),
        }),
        outcome,
        request: Some(RequestContext {
            id: req_id,
            method: method.to_string(),
            path: path.to_string(),
            ip: client_ip(headers, peer),
            user_agent: user_agent(headers),
        }),
        correlation_id: correlation_from_headers(headers),
        reason,
        metadata,
    };

    sentry::add_breadcrumb(sentry::Breadcrumb {
        category: Some("audit".into()),
        message: Some("audit.append_request_serialized".into()),
        level: sentry::Level::Info,
        ..Default::default()
    });

    let mut req = Request::new(AppendAuditEventRequest { event: Some(event) });
    req.set_timeout(audit_append_deadline());

    let res = append_with_client(client, req).await;
    match res {
        Ok(id) => {
            sentry::add_breadcrumb(sentry::Breadcrumb {
                category: Some("audit".into()),
                message: Some(format!("audit.append_response audit_event_id={id}")),
                level: sentry::Level::Info,
                ..Default::default()
            });
        }
        Err(e) => {
            let cid = correlation_from_headers(headers);
            tracing::error!(
                correlation_id = %cid,
                audit.action = action,
                source_service = "users",
                grpc_status = %e,
                "audit append failed after users mutation"
            );
            sentry::add_breadcrumb(sentry::Breadcrumb {
                category: Some("audit".into()),
                message: Some(format!(
                    "audit.append_failed action={action} correlation_id={cid} source_service=users grpc={e}"
                )),
                level: sentry::Level::Error,
                ..Default::default()
            });
            sentry::configure_scope(|scope| {
                scope.set_tag("source_service", "users");
                scope.set_tag("audit.action", action);
                scope.set_tag("correlation_id", cid.as_str());
            });
            sentry::capture_message(
                &format!("audit-append-failure: {e}"),
                sentry::Level::Error,
            );
        }
    }
    txn.finish();
}

async fn append_with_client(
    client: &AuditServiceClient<Channel>,
    req: Request<AppendAuditEventRequest>,
) -> Result<String, tonic::Status> {
    let resp = client.clone().append_audit_event(req).await?;
    Ok(resp.into_inner().audit_event_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::audit_proto::audit_service_server::{AuditService, AuditServiceServer};
    use crate::grpc::GrpcClients;
    use crate::grpc::audit_proto::{AppendAuditEventRequest, AppendAuditEventResponse};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::{Endpoint, Server};

    struct CountingAudit {
        hits: Arc<AtomicUsize>,
    }

    #[tonic::async_trait]
    impl AuditService for CountingAudit {
        async fn append_audit_event(
            &self,
            _request: tonic::Request<AppendAuditEventRequest>,
        ) -> Result<tonic::Response<AppendAuditEventResponse>, tonic::Status> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(tonic::Response::new(AppendAuditEventResponse {
                audit_event_id: Uuid::new_v4().to_string(),
            }))
        }
    }

    #[tokio::test]
    async fn emit_invokes_grpc_when_client_configured() {
        let hits = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let svc = CountingAudit {
            hits: hits.clone(),
        };
        let server = Server::builder()
            .add_service(AuditServiceServer::new(svc))
            .serve_with_incoming_shutdown(incoming, async {
                let _ = rx.await;
            });
        let j = tokio::spawn(server);
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;

        let endpoint = format!("http://{addr}");
        let ch = Endpoint::from_shared(endpoint.clone())
            .unwrap()
            .connect_lazy();
        let client = AuditServiceClient::new(ch);
        let grpc = GrpcClients::new(None, Some(client));

        let mut headers = HeaderMap::new();
        headers.insert("x-correlation-id", "cid-test".parse().unwrap());
        let peer = SocketAddr::from(([127, 0, 0, 1], 9));

        emit_users_mutation(
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
            Uuid::nil(),
            401,
            None,
            HashMap::new(),
        )
        .await;

        assert!(hits.load(Ordering::SeqCst) >= 1);
        let _ = tx.send(());
        let _ = j.await;
    }

    #[tokio::test]
    async fn emit_skips_when_no_client() {
        let grpc = GrpcClients::none();
        let headers = HeaderMap::new();
        let peer = SocketAddr::from(([127, 0, 0, 1], 1));
        emit_users_mutation(
            &grpc,
            &headers,
            &peer,
            "POST",
            "/x",
            "users.auth.login",
            Uuid::nil(),
            ActorType::Anonymous,
            "",
            vec![],
            "user",
            Uuid::nil(),
            200,
            None,
            HashMap::new(),
        )
        .await;
    }
}
