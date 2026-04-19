use accounts_api::models::Transaction;
use accounts_api::repositories::TransactionRepository;
use chrono::Duration;
use sqlx::PgPool;

pub async fn claim_for_processing(
    pool: &PgPool,
    environment: Option<&str>,
) -> Vec<Transaction> {
    TransactionRepository::claim_pending_transactions_for_ledger_post(
        pool,
        Duration::seconds(1),
        Duration::seconds(300),
        Some(10),
        environment,
    )
    .await
    .unwrap()
}

pub async fn claim_one_for_processing(pool: &PgPool, environment: Option<&str>) -> Transaction {
    let mut rows = claim_for_processing(pool, environment).await;
    assert_eq!(rows.len(), 1, "expected exactly one claimed row");
    rows.pop().unwrap()
}

pub async fn claim_stale_posting_batch(
    pool: &PgPool,
    environment: Option<&str>,
) -> Vec<Transaction> {
    TransactionRepository::claim_pending_transactions_for_ledger_post(
        pool,
        Duration::seconds(1),
        Duration::seconds(10),
        Some(10),
        environment,
    )
    .await
    .unwrap()
}

pub async fn claim_with_default_limit(pool: &PgPool) -> Vec<Transaction> {
    TransactionRepository::claim_pending_transactions_for_ledger_post(
        pool,
        Duration::seconds(1),
        Duration::seconds(300),
        None,
        None,
    )
    .await
    .unwrap()
}
