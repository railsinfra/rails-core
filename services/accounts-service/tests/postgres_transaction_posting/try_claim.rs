use accounts_api::models::TransactionStatus;
use accounts_api::repositories::TransactionRepository;
use chrono::Duration;
use uuid::Uuid;

use crate::support::{insert_pending_deposit, migrated_pool};

#[tokio::test]
async fn try_claim_pending_then_none_when_not_pending() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-{}", Uuid::new_v4()),
        "sandbox",
        Duration::seconds(10),
    )
    .await;

    let claimed = TransactionRepository::try_claim_pending_for_post(&pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.status, TransactionStatus::Posting);

    let second = TransactionRepository::try_claim_pending_for_post(&pool, id)
        .await
        .unwrap();
    assert!(second.is_none());
}

#[tokio::test]
async fn try_claim_pending_returns_none_for_unknown_id() {
    let (_c, pool) = migrated_pool().await;
    let r = TransactionRepository::try_claim_pending_for_post(&pool, Uuid::new_v4())
        .await
        .unwrap();
    assert!(r.is_none());
}
