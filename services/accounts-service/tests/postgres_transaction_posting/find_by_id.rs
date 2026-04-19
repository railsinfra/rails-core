use accounts_api::errors::AppError;
use accounts_api::repositories::TransactionRepository;
use uuid::Uuid;

use crate::support::{
    drop_transaction_kind_check, drop_transaction_status_check, hours_ago,
    insert_pending_row_bypassing_checks, migrated_pool,
};

#[tokio::test]
async fn find_by_id_rejects_invalid_transaction_kind_after_constraint_drop() {
    let (_c, pool) = migrated_pool().await;
    drop_transaction_kind_check(&pool).await;

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-bad-kind-{}", Uuid::new_v4());
    insert_pending_row_bypassing_checks(
        &pool,
        org,
        acc,
        id,
        &idem,
        "not_a_kind",
        "pending",
        hours_ago(1),
    )
    .await;

    let err = TransactionRepository::find_by_id(&pool, id).await.unwrap_err();
    match err {
        AppError::Internal(msg) => assert!(msg.contains("Invalid transaction kind"), "{msg}"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn find_by_id_rejects_invalid_transaction_status_after_constraint_drop() {
    let (_c, pool) = migrated_pool().await;
    drop_transaction_status_check(&pool).await;

    let org = Uuid::new_v4();
    let id = Uuid::new_v4();
    let acc = Uuid::new_v4();
    let idem = format!("idem-bad-status-{}", Uuid::new_v4());
    insert_pending_row_bypassing_checks(
        &pool,
        org,
        acc,
        id,
        &idem,
        "deposit",
        "bogus_status",
        hours_ago(1),
    )
    .await;

    let err = TransactionRepository::find_by_id(&pool, id).await.unwrap_err();
    match err {
        AppError::Internal(msg) => assert!(msg.contains("Invalid transaction status"), "{msg}"),
        other => panic!("unexpected error: {other:?}"),
    }
}
