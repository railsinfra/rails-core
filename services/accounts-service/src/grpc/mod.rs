pub mod accounts;

pub mod proto {
    tonic::include_proto!("rails.accounts.v1");
}

pub mod ledger_proto {
    tonic::include_proto!("rails.ledger.v1");
}
