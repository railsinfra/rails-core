use accounts_api::models::TransactionStatus;
use chrono::Duration;
use uuid::Uuid;

use crate::support::{
    claim_for_processing, claim_stale_posting_batch, claim_with_default_limit, insert_pending_deposit,
    mark_posting_stale_30s, migrated_pool,
};

#[tokio::test]
async fn claim_batch_reclaims_stale_posting_then_claims_that_row() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-stale-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(3),
    )
    .await;

    mark_posting_stale_30s(&pool, id).await;

    let claimed = claim_stale_posting_batch(&pool, None).await;

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id);
    assert_eq!(claimed[0].status, TransactionStatus::Posting);
}

#[tokio::test]
async fn claim_batch_reclaims_stale_posting_with_sandbox_env_filter() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-stale-sb-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(3),
    )
    .await;

    mark_posting_stale_30s(&pool, id).await;

    let claimed = claim_stale_posting_batch(&pool, Some("sandbox")).await;

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id);
    assert_eq!(claimed[0].status, TransactionStatus::Posting);
}

#[tokio::test]
async fn claim_batch_respects_environment_filter() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    let id_sb = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-sb-{}", Uuid::new_v4()),
        "sandbox",
        Duration::hours(2),
    )
    .await;
    let _id_prod = insert_pending_deposit(
        &pool,
        org,
        &format!("idem-pr-{}", Uuid::new_v4()),
        "production",
        Duration::hours(2),
    )
    .await;

    let claimed = claim_for_processing(&pool, Some("sandbox")).await;

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, id_sb);
    assert_eq!(claimed[0].environment.as_deref(), Some("sandbox"));
}

#[tokio::test]
async fn claim_pending_default_limit_branch() {
    let (_c, pool) = migrated_pool().await;
    let org = Uuid::new_v4();
    for i in 0..3 {
        insert_pending_deposit(
            &pool,
            org,
            &format!("idem-many-{i}-{}", Uuid::new_v4()),
            "sandbox",
            Duration::hours(2),
        )
        .await;
    }
    let claimed = claim_with_default_limit(&pool).await;
    assert_eq!(claimed.len(), 3);
}
