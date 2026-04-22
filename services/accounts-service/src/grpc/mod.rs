pub mod accounts;

pub mod proto {
    tonic::include_proto!("rails.accounts.v1");
}

pub mod ledger_proto {
    tonic::include_proto!("rails.ledger.v1");
}

pub mod audit_proto {
    tonic::include_proto!("rails.core.audit.v1");
}

use audit_proto::audit_service_client::AuditServiceClient;
use tonic::transport::{Channel, Endpoint};

pub fn audit_channel(url: &str) -> Option<AuditServiceClient<Channel>> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let endpoint = Endpoint::from_shared(url.to_string()).ok()?.connect_lazy();
    Some(AuditServiceClient::new(endpoint))
}
