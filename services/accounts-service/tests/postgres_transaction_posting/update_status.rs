use accounts_api::models::TransactionStatus;
use accounts_api::repositories::TransactionRepository;
use chrono::Duration;
use uuid::Uuid;

use crate::support::{insert_pending_deposit, migrated_pool};

#[tokio::test]
async fn update_status_can_set_posting() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-posting-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(1),
    )
    .await;

    let claimed = TransactionRepository::try_claim_pending_for_post(&pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.retry_count, 1);
    assert!(claimed.last_attempted_at.is_some());

    let updated = TransactionRepository::update_status(
        &pool,
        id,
        TransactionStatus::Posting,
        None,
    )
    .await
    .unwrap();
    assert_eq!(updated.status, TransactionStatus::Posting);
    assert_eq!(updated.retry_count, 1);
    assert!(updated.last_attempted_at.is_some());
}
