use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Deserializer};
use uuid::Uuid;
use crate::errors::AppError;
use crate::models::{
    AccountResponse, AccountTransactionResponse, CreateAccountRequest, PaginatedAccountsResponse,
    UpdateAccountRequest,
};
use crate::routes::api::AppState;
use crate::services::AccountService;

/// Extract and validate environment from X-Environment header.
/// Returns error if missing or not one of sandbox/production (no defaults).
pub(crate) fn extract_environment(headers: &HeaderMap) -> Result<String, AppError> {
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
pub struct ListAccountsQuery {
    pub user_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
    pub admin_user_id: Option<Uuid>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Ledger stores liability (customer) balances as negative.
/// Negate for customer-facing display: positive = money they have.
/// Returns balance in cents (i64). Returns 0 when input is empty or unparseable.
pub(crate) fn negate_ledger_balance_for_display(balance: &str) -> i64 {
    let trimmed = balance.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed.parse::<i64>().map(|n| -n).unwrap_or(0)
}

/// Extract X-API-Key from headers. Returns None if missing or empty.
pub(crate) fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AppError> {
    let environment = extract_environment(&headers)?;
    let mut request = request;
    request.environment = Some(environment.clone());

    // Holder-based path: email (and optionally first_name, last_name) + X-API-Key
    let is_holder_path = request
        .email
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let account = if is_holder_path {
        let api_key = extract_api_key(&headers)
            .ok_or_else(|| AppError::Validation("X-API-Key header is required for holder-based account creation".to_string()))?;
        let (organization_id, _environment_id, admin_user_id) = state
            .users_grpc
            .validate_api_key(&api_key, &environment)
            .await?;
        AccountService::create_account_with_holder(
            &state.pool,
            request,
            organization_id,
            Some(admin_user_id),
        )
        .await?
    } else {
        AccountService::create_account(&state.pool, request).await?
    };

    Ok((StatusCode::CREATED, Json(account.into())))
}

pub async fn get_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AccountResponse>, AppError> {
    let environment = extract_environment(&headers)?;
    let account = AccountService::get_account(&state.pool, id, &environment).await?;
    let mut resp = AccountResponse::from(account.clone());

    if let Some(org_id) = account.organization_id {
        let currency = account.currency.as_deref().unwrap_or("USD");
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        resp = resp.with_balance(display_balance);
    } else {
        resp = resp.with_balance(0);
    }

    Ok(Json(resp))
}

pub async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListAccountsQuery>,
) -> Result<Json<PaginatedAccountsResponse>, AppError> {
    // Extract environment from header (defaults to sandbox if missing)
    let environment = extract_environment(&headers)?;
    
    // Parse and validate pagination params with defaults
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(10).min(100).max(1);

    // Support three filtering options:
    // 1. user_id: Get accounts owned by a specific user
    // 2. organization_id: Get all accounts in an organization (for admins)
    // 3. admin_user_id: Get accounts managed by an admin (customer accounts)
    let result = if let Some(user_id) = query.user_id {
        AccountService::get_accounts_by_user_paginated(&state.pool, user_id, &environment, page, per_page).await?
    } else if let Some(organization_id) = query.organization_id {
        AccountService::get_accounts_by_organization_paginated(&state.pool, organization_id, &environment, page, per_page).await?
    } else if let Some(admin_user_id) = query.admin_user_id {
        AccountService::get_accounts_by_admin_paginated(&state.pool, admin_user_id, &environment, page, per_page).await?
    } else {
        return Err(AppError::Validation(
            "One of user_id, organization_id, or admin_user_id query parameter is required".to_string()
        ));
    };

    Ok(Json(result))
}

pub async fn update_account_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateAccountRequest>,
) -> Result<Json<AccountResponse>, AppError> {
    let environment = extract_environment(&headers)?;
    
    let status = request.status.ok_or_else(|| {
        AppError::Validation("status field is required".to_string())
    })?;

    let account = AccountService::update_account_status(&state.pool, id, &environment, status).await?;
    Ok(Json(account.into()))
}

pub async fn close_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AccountResponse>, AppError> {
    let environment = extract_environment(&headers)?;
    let account = AccountService::close_account(&state.pool, id, &environment).await?;
    Ok(Json(account.into()))
}

pub async fn deposit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<crate::handlers::accounts::DepositRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation("Amount must be greater than zero".to_string()));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());

    let (account, transaction) = AccountService::deposit_with_idempotency(
        &state.pool,
        id,
        &environment,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id,
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await?;

    let mut account_resp = AccountResponse::from(account.clone());
    if let Some(org_id) = account.organization_id {
        let currency = account.currency.as_deref().unwrap_or("USD");
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        account_resp = account_resp.with_balance(display_balance);
    }

    let txn_resp = AccountTransactionResponse::for_mutation_response(&transaction, account_resp.balance);
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "account": account_resp,
            "transaction": txn_resp
        })),
    ))
}

pub async fn withdraw(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<crate::handlers::accounts::WithdrawRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation("Amount must be greater than zero".to_string()));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());

    let (account, transaction) = AccountService::withdraw_with_idempotency(
        &state.pool,
        id,
        &environment,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id,
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await?;

    let mut account_resp = AccountResponse::from(account.clone());
    if let Some(org_id) = account.organization_id {
        let currency = account.currency.as_deref().unwrap_or("USD");
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        account_resp = account_resp.with_balance(display_balance);
    }

    let txn_resp = AccountTransactionResponse::for_mutation_response(&transaction, account_resp.balance);
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "account": account_resp,
            "transaction": txn_resp
        })),
    ))
}

pub async fn transfer(
    State(state): State<AppState>,
    Path(from_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<crate::handlers::accounts::TransferRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation("Amount must be greater than zero".to_string()));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());

    let (from_account, to_account, transaction) = AccountService::transfer_with_idempotency(
        &state.pool,
        from_id,
        &environment,
        request.to_account_id,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id,
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await?;

    let mut from_resp = AccountResponse::from(from_account.clone());
    let mut to_resp = AccountResponse::from(to_account.clone());
    if let Some(org_id) = from_account.organization_id {
        let currency = from_account.currency.as_deref().unwrap_or("USD");
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, from_account.id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %from_account.id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        from_resp = from_resp.with_balance(display_balance);
    }
    if let Some(org_id) = to_account.organization_id {
        let currency = to_account.currency.as_deref().unwrap_or("USD");
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, to_account.id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %to_account.id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        to_resp = to_resp.with_balance(display_balance);
    }

    let txn_resp = AccountTransactionResponse::for_mutation_response(&transaction, from_resp.balance);
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "from_account": from_resp,
            "to_account": to_resp,
            "transaction": txn_resp
        })),
    ))
}

/// Deserialize amount from JSON number or string (e.g. 10000 or "10000").
/// Amount is in minor units (cents). Strings like "100.00" are parsed as dollars and converted to cents.
fn deserialize_amount<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => {
            n.as_i64().ok_or_else(|| D::Error::custom("amount must be a valid integer"))
        }
        serde_json::Value::String(s) => {
            let s = s.trim();
            if let Ok(n) = s.parse::<i64>() {
                Ok(n)
            } else if let Ok(f) = s.parse::<f64>() {
                Ok((f * 100.0).round() as i64)
            } else {
                Err(D::Error::custom("amount must be a number or numeric string"))
            }
        }
        _ => Err(D::Error::custom("amount must be a number or string")),
    }
}

#[derive(Deserialize)]
pub struct DepositRequest {
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: i64,
    pub description: Option<String>,
    pub external_recipient_id: Option<String>,
    pub reference_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct WithdrawRequest {
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: i64,
    pub description: Option<String>,
    pub external_recipient_id: Option<String>,
    pub reference_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct TransferRequest {
    pub to_account_id: Uuid,
    #[serde(deserialize_with = "deserialize_amount")]
    pub amount: i64,
    pub description: Option<String>,
    pub external_recipient_id: Option<String>,
    pub reference_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negate_ledger_balance_negates_negative() {
        assert_eq!(negate_ledger_balance_for_display("-40000"), 40000);
        assert_eq!(negate_ledger_balance_for_display("-100"), 100);
    }

    #[test]
    fn negate_ledger_balance_negates_positive() {
        assert_eq!(negate_ledger_balance_for_display("1000"), -1000);
    }

    #[test]
    fn negate_ledger_balance_handles_zero() {
        assert_eq!(negate_ledger_balance_for_display("0"), 0);
    }

    #[test]
    fn negate_ledger_balance_handles_trim() {
        assert_eq!(negate_ledger_balance_for_display("  -40000  "), 40000);
    }

    #[test]
    fn negate_ledger_balance_handles_invalid_returns_zero() {
        assert_eq!(negate_ledger_balance_for_display("invalid"), 0);
        assert_eq!(negate_ledger_balance_for_display(""), 0);
    }

    #[test]
    fn deserialize_amount_number() {
        let json = r#"{"amount": 10000}"#;
        let req: DepositRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.amount, 10000);
    }

    #[test]
    fn deserialize_amount_string_int() {
        let json = r#"{"amount": "10000"}"#;
        let req: DepositRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.amount, 10000);
    }

    #[test]
    fn deserialize_amount_string_decimal_converts_to_cents() {
        let json = r#"{"amount": "100.00"}"#;
        let req: DepositRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.amount, 10000);
    }

    #[test]
    fn deserialize_amount_transfer_request() {
        let json = r#"{"to_account_id": "550e8400-e29b-41d4-a716-446655440000", "amount": "50.50"}"#;
        let req: TransferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.amount, 5050);
    }
}
