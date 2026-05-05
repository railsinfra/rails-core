use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::accounts::negate_ledger_balance_for_display;
use crate::models::{AccountTransactionResponse, PaginatedTransactionsResponse};
use crate::routes::api::AppState;
use crate::services::TransactionService;

/// Extract and validate environment from X-Environment header.
/// Returns error if missing or not one of sandbox/production (no defaults).
fn extract_environment(headers: &HeaderMap) -> Result<String, AppError> {
    let env = headers
        .get("x-environment")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_lowercase());
    match env.as_deref() {
        Some("sandbox") | Some("production") => Ok(env.unwrap()),
        _ => Err(AppError::Validation(
            "X-Environment header is required and must be 'sandbox' or 'production'".to_string(),
        )),
    }
}

#[derive(Deserialize)]
pub struct ListTransactionsQuery {
    pub limit: Option<i64>,
    pub organization_id: Option<Uuid>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

pub async fn get_transaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AccountTransactionResponse>, AppError> {
    let environment = extract_environment(&headers)?;
    let transaction = TransactionService::get_transaction(&state.pool, id, &environment).await?;

    let balance_after = match state
        .ledger_grpc
        .get_account_balance(
            transaction.organization_id,
            &environment,
            transaction.from_account_id,
            &transaction.currency,
        )
        .await
    {
        Ok(balance) => negate_ledger_balance_for_display(&balance),
        Err(e) => {
            tracing::warn!(
                transaction_id = %id,
                error = %e,
                "Failed to fetch balance from Ledger, using 0"
            );
            0
        }
    };

    let resp = AccountTransactionResponse::for_mutation_response(&transaction, balance_after);
    Ok(Json(resp))
}

/// Balance change for an account from a transaction. Positive = balance increased.
/// Only Posted transactions affect balance; Pending/Failed return 0.
pub(crate) fn balance_delta_for_account(tx: &crate::models::Transaction, account_id: Uuid) -> i64 {
    if tx.status != crate::models::TransactionStatus::Posted {
        return 0;
    }
    match tx.transaction_kind {
        crate::models::TransactionKind::Deposit => tx.amount,
        crate::models::TransactionKind::Withdraw => -tx.amount,
        crate::models::TransactionKind::Transfer => {
            if tx.from_account_id == account_id {
                -tx.amount
            } else {
                tx.amount
            }
        }
    }
}

pub async fn list_account_transactions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<Uuid>,
    Query(query): Query<ListTransactionsQuery>,
) -> Result<Json<Vec<AccountTransactionResponse>>, AppError> {
    let environment = extract_environment(&headers)?;

    let transactions = TransactionService::get_account_transactions(
        &state.pool,
        account_id,
        &environment,
        query.limit,
    )
    .await?;

    let current_balance = match transactions.first() {
        Some(tx) => {
            match state
                .ledger_grpc
                .get_account_balance(
                    tx.organization_id,
                    &environment,
                    account_id,
                    &tx.currency,
                )
                .await
            {
                Ok(balance) => negate_ledger_balance_for_display(&balance),
                Err(e) => {
                    tracing::warn!(
                        account_id = %account_id,
                        error = %e,
                        "Failed to fetch balance from Ledger, using 0"
                    );
                    0
                }
            }
        }
        None => 0,
    };

    // Compute balance_after for each transaction by working backwards from current balance.
    // Transactions are ordered newest first (created_at DESC).
    let mut balance_after_per_tx: Vec<i64> = Vec::with_capacity(transactions.len());
    let mut running = current_balance;
    for tx in &transactions {
        balance_after_per_tx.push(running);
        running -= balance_delta_for_account(tx, account_id);
    }

    Ok(Json(
        transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| {
                let balance_after = balance_after_per_tx.get(i).copied().unwrap_or(0);
                AccountTransactionResponse::from_transaction(tx, account_id, balance_after)
            })
            .collect(),
    ))
}

pub async fn list_transactions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListTransactionsQuery>,
) -> Result<Json<PaginatedTransactionsResponse>, AppError> {
    let environment = extract_environment(&headers)?;

    let organization_id = query.organization_id.ok_or_else(|| {
        AppError::Validation("organization_id query parameter is required".to_string())
    })?;

    // Parse and validate pagination params with defaults
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(10).clamp(1, 100);

    let (transactions, pagination) = TransactionService::get_transactions_by_organization_paginated(
        &state.pool,
        organization_id,
        &environment,
        page,
        per_page,
    )
    .await?;

    // Compute historical balance_after per transaction (same logic as list_account_transactions).
    // Transactions are newest first; walk backwards from current Ledger balance.
    use std::collections::{HashMap, HashSet};
    let mut unique_accounts: HashSet<(Uuid, String)> = HashSet::new();
    for tx in &transactions {
        unique_accounts.insert((tx.from_account_id, tx.currency.clone()));
        if tx.transaction_kind == crate::models::TransactionKind::Transfer {
            unique_accounts.insert((tx.to_account_id, tx.currency.clone()));
        }
    }

    let mut balance_cache: HashMap<Uuid, i64> = HashMap::new();
    for (account_id, currency) in &unique_accounts {
        if balance_cache.contains_key(account_id) {
            continue;
        }
        let balance = match state
            .ledger_grpc
            .get_account_balance(
                organization_id,
                &environment,
                *account_id,
                currency,
            )
            .await
        {
            Ok(b) => negate_ledger_balance_for_display(&b),
            Err(e) => {
                tracing::warn!(
                    account_id = %account_id,
                    error = %e,
                    "Failed to fetch balance from Ledger, using 0"
                );
                0
            }
        };
        balance_cache.insert(*account_id, balance);
    }

    // Walk backwards from current balance; for transfers, capture balance_after for both sides.
    type BalancePair = (i64, Option<i64>); // (from_balance_after, to_balance_after for transfers)
    let mut balance_after_per_tx: Vec<BalancePair> = Vec::with_capacity(transactions.len());
    for tx in &transactions {
        let from_ba = *balance_cache.get(&tx.from_account_id).unwrap_or(&0);
        let to_ba = if tx.transaction_kind == crate::models::TransactionKind::Transfer {
            Some(*balance_cache.get(&tx.to_account_id).unwrap_or(&0))
        } else {
            None
        };
        balance_after_per_tx.push((from_ba, to_ba));
        let delta_from = balance_delta_for_account(tx, tx.from_account_id);
        balance_cache.insert(tx.from_account_id, from_ba - delta_from);
        if tx.transaction_kind == crate::models::TransactionKind::Transfer {
            let to_delta = balance_delta_for_account(tx, tx.to_account_id);
            let to_curr = *balance_cache.get(&tx.to_account_id).unwrap_or(&0);
            balance_cache.insert(tx.to_account_id, to_curr - to_delta);
        }
    }

    // Industry standard: transfers appear as two rows (sender + recipient), each with balance_after.
    let mut data: Vec<AccountTransactionResponse> = Vec::new();
    for (i, tx) in transactions.iter().enumerate() {
        let (from_ba, to_ba) = balance_after_per_tx.get(i).copied().unwrap_or((0, None));
        match tx.transaction_kind {
            crate::models::TransactionKind::Transfer => {
                data.push(AccountTransactionResponse::from_transaction(tx, tx.from_account_id, from_ba));
                if let Some(ba) = to_ba {
                    data.push(AccountTransactionResponse::from_transaction(tx, tx.to_account_id, ba));
                }
            }
            _ => {
                data.push(AccountTransactionResponse::for_mutation_response(tx, from_ba));
            }
        }
    }

    Ok(Json(PaginatedTransactionsResponse { data, pagination }))
}
