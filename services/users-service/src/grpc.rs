use crate::config::Config;
use tonic::transport::{Channel, Endpoint};

pub mod proto {
    tonic::include_proto!("rails.accounts.v1");
}

pub mod audit_proto {
    tonic::include_proto!("rails.core.audit.v1");
}

use audit_proto::audit_service_client::AuditServiceClient;
use proto::accounts_service_client::AccountsServiceClient;

#[derive(Clone)]
pub struct GrpcClients {
    pub(crate) accounts_client: Option<AccountsServiceClient<Channel>>,
    pub(crate) audit_client: Option<AuditServiceClient<Channel>>,
}

impl GrpcClients {
    pub fn none() -> Self {
        Self {
            accounts_client: None,
            audit_client: None,
        }
    }

    /// Construct gRPC clients directly (e.g. integration tests).
    pub fn new(
        accounts_client: Option<AccountsServiceClient<Channel>>,
        audit_client: Option<AuditServiceClient<Channel>>,
    ) -> Self {
        Self {
            accounts_client,
            audit_client,
        }
    }

    fn audit_channel(url: &str) -> Option<AuditServiceClient<Channel>> {
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        Endpoint::from_shared(url.to_string())
            .ok()
            .map(|e| AuditServiceClient::new(e.connect_lazy()))
    }
}

pub async fn init(config: &Config) -> Result<GrpcClients, tonic::transport::Error> {
    let accounts_client = match AccountsServiceClient::connect(config.accounts_grpc_url.clone()).await
    {
        Ok(client) => {
            tracing::info!(
                "Connected to Accounts gRPC service at {}",
                config.accounts_grpc_url
            );
            Some(client)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to Accounts gRPC service at {}: {}",
                config.accounts_grpc_url,
                e
            );
            tracing::warn!("Account creation will fail - ensure Accounts service is running");
            None
        }
    };

    let audit_client = GrpcClients::audit_channel(&config.audit_grpc_url);
    if audit_client.is_some() {
        tracing::info!(
            "Audit gRPC client configured (lazy) at {}",
            config.audit_grpc_url
        );
    } else {
        tracing::warn!("AUDIT_GRPC_URL empty — audit append disabled");
    }

    Ok(GrpcClients {
        accounts_client,
        audit_client,
    })
}

#[cfg(test)]
mod tests {
    use super::{init, proto, GrpcClients};
    use crate::config::Config;
    use proto::accounts_service_server::{AccountsService, AccountsServiceServer};
    use proto::{GetAccountBalanceRequest, GetAccountBalanceResponse};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;

    struct MockAccounts;

    #[tonic::async_trait]
    impl AccountsService for MockAccounts {
        async fn get_account_balance(
            &self,
            _request: tonic::Request<GetAccountBalanceRequest>,
        ) -> Result<tonic::Response<GetAccountBalanceResponse>, tonic::Status> {
            Ok(tonic::Response::new(GetAccountBalanceResponse {
                account_id: "acct".into(),
                balance: "0".into(),
                currency: "ZAR".into(),
            }))
        }
    }

    #[tokio::test]
    async fn init_treats_connection_refused_as_optional_accounts_client() {
        let mut cfg = Config::test_stub_with_accounts_grpc("http://127.0.0.1:1".into());
        cfg.audit_grpc_url = "http://127.0.0.1:1".into();
        let clients = init(&cfg).await.expect("init returns Ok even when connect fails");
        assert!(clients.accounts_client.is_none());
    }

    #[tokio::test]
    async fn init_connects_when_accounts_grpc_is_reachable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let incoming = TcpListenerStream::new(listener);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = Server::builder()
            .add_service(AccountsServiceServer::new(MockAccounts))
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            });
        let join = tokio::spawn(server);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let mut cfg = Config::test_stub_with_accounts_grpc(format!("http://{}", addr));
        cfg.audit_grpc_url = "http://127.0.0.1:1".into();
        let clients = init(&cfg).await.expect("init");
        assert!(clients.accounts_client.is_some());
        let _ = shutdown_tx.send(());
        let _ = join.await;
    }

    #[test]
    fn audit_channel_empty_string_is_none() {
        assert!(GrpcClients::audit_channel("").is_none());
        assert!(GrpcClients::audit_channel("   ").is_none());
    }

    #[tokio::test]
    async fn init_skips_audit_client_when_audit_url_empty() {
        let mut cfg = Config::test_stub_with_accounts_grpc("http://127.0.0.1:1".into());
        cfg.audit_grpc_url.clear();
        let clients = init(&cfg).await.expect("init");
        assert!(clients.audit_client.is_none());
    }
}
