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

    let updated = TransactionRepository::update_status(
        &pool,
        id,
        TransactionStatus::Posting,
        None,
    )
    .await
    .unwrap();
    assert_eq!(updated.status, TransactionStatus::Posting);
}
