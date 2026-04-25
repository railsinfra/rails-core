use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use tonic::transport::Channel;
use tracing::Level;
use uuid::Uuid;

use crate::errors::AppError;
use crate::grpc::audit_proto::audit_service_client::AuditServiceClient;
use crate::grpc::audit_proto::ActorType;
use crate::models::{
    Account, AccountResponse, AccountTransactionResponse, CreateAccountRequest,
    PaginatedAccountsResponse, UpdateAccountRequest,
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

fn log_money_request_boundary(
    phase: &str,
    operation: &str,
    path: &str,
    correlation_id: Option<&str>,
    idempotency_key: &str,
    status: Option<u16>,
    elapsed_ms: Option<u64>,
) {
    let banner = format!(
        "-----------------------[{phase} - {}]----------------------------",
        operation.to_uppercase()
    );
    tracing::info!(
        target = "accounts.request_boundary",
        marker = %banner,
        operation,
        path,
        correlation_id = correlation_id.unwrap_or("missing"),
        idempotency_key,
        status = status.unwrap_or(0),
        elapsed_ms = elapsed_ms.unwrap_or(0),
        "{banner}"
    );
}

fn spawn_audit_emit(
    audit_client: Option<AuditServiceClient<Channel>>,
    headers: HeaderMap,
    peer: SocketAddr,
    method: &'static str,
    path: String,
    action: &'static str,
    organization_id: Uuid,
    actor_type: ActorType,
    actor_id: String,
    actor_roles: Vec<String>,
    target_type: &'static str,
    target_id: Uuid,
    http_status: u16,
    reason: Option<String>,
    metadata: HashMap<String, String>,
) {
    tokio::spawn(async move {
        let started_at = Instant::now();
        crate::audit_emit::emit_accounts_mutation(
            &audit_client,
            &headers,
            &peer,
            method,
            &path,
            action,
            organization_id,
            actor_type,
            &actor_id,
            actor_roles,
            target_type,
            target_id,
            http_status,
            reason,
            metadata,
        )
        .await;
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        tracing::event!(
            Level::INFO,
            target = "accounts.latency",
            action = action,
            path = path.as_str(),
            elapsed_ms,
            "audit_emit_timing_background"
        );
    });
}

pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AppError> {
    let path = "/api/v1/accounts";
    let environment = extract_environment(&headers)?;
    let mut request = request;
    request.environment = Some(environment.clone());

    // Holder-based path: email (and optionally first_name, last_name) + X-API-Key
    let is_holder_path = request
        .email
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let org_hint_non_holder = request.organization_id;
    let mut holder_org_on_err: Option<Uuid> = None;

    let account_res: Result<Account, AppError> = if is_holder_path {
        let api_key = extract_api_key(&headers).ok_or_else(|| {
            AppError::Validation(
                "X-API-Key header is required for holder-based account creation".to_string(),
            )
        })?;
        match state
            .users_grpc
            .validate_api_key(&api_key, &environment)
            .await
        {
            Ok((organization_id, _environment_id, admin_user_id)) => {
                holder_org_on_err = Some(organization_id);
                AccountService::create_account_with_holder(
                    &state.pool,
                    request,
                    organization_id,
                    Some(admin_user_id),
                )
                .await
            }
            Err(e) => Err(e),
        }
    } else {
        AccountService::create_account(&state.pool, request).await
    };

    let mut meta = HashMap::default();
    match &account_res {
        Ok(account) => {
            if let Some(org) = account.organization_id {
                let (actor_type, actor_id) = if is_holder_path {
                    let aid = account
                        .admin_user_id
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    (ActorType::User, aid)
                } else if let Some(admin) = account.admin_user_id {
                    (ActorType::User, admin.to_string())
                } else if let Some(uid) = account.user_id {
                    (ActorType::User, uid.to_string())
                } else {
                    (ActorType::Anonymous, String::new())
                };
                spawn_audit_emit(
                    state.audit_client.clone(),
                    headers.clone(),
                    peer,
                    "POST",
                    path.to_string(),
                    "accounts.account.create",
                    org,
                    actor_type,
                    actor_id,
                    vec![],
                    "account",
                    account.id,
                    201,
                    None,
                    meta,
                );
            }
        }
        Err(e) => {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
            let org_opt = if is_holder_path {
                holder_org_on_err
            } else {
                org_hint_non_holder
            };
            if let Some(org) = org_opt {
                spawn_audit_emit(
                    state.audit_client.clone(),
                    headers.clone(),
                    peer,
                    "POST",
                    path.to_string(),
                    "accounts.account.create",
                    org,
                    ActorType::Anonymous,
                    String::new(),
                    vec![],
                    "account",
                    Uuid::nil(),
                    crate::audit_emit::http_status_for_error(e),
                    Some(crate::audit_emit::truncate_reason(&e.to_string())),
                    meta,
                );
            }
        }
    }

    let account = account_res?;
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
        AccountService::get_accounts_by_user_paginated(
            &state.pool,
            user_id,
            &environment,
            page,
            per_page,
        )
        .await?
    } else if let Some(organization_id) = query.organization_id {
        AccountService::get_accounts_by_organization_paginated(
            &state.pool,
            organization_id,
            &environment,
            page,
            per_page,
        )
        .await?
    } else if let Some(admin_user_id) = query.admin_user_id {
        AccountService::get_accounts_by_admin_paginated(
            &state.pool,
            admin_user_id,
            &environment,
            page,
            per_page,
        )
        .await?
    } else {
        return Err(AppError::Validation(
            "One of user_id, organization_id, or admin_user_id query parameter is required"
                .to_string(),
        ));
    };

    Ok(Json(result))
}

pub async fn update_account_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateAccountRequest>,
) -> Result<Json<AccountResponse>, AppError> {
    let environment = extract_environment(&headers)?;

    let status = request
        .status
        .ok_or_else(|| AppError::Validation("status field is required".to_string()))?;

    let path = format!("/api/v1/accounts/{id}");
    let result = AccountService::update_account_status(&state.pool, id, &environment, status).await;
    let org = match &result {
        Ok(account) => account.organization_id,
        Err(_) => AccountService::get_account(&state.pool, id, &environment)
            .await
            .ok()
            .and_then(|a| a.organization_id),
    };
    if let Some(org) = org {
        let mut meta = HashMap::default();
        if result.is_ok() {
            meta.insert("new_status".into(), format!("{status:?}"));
        } else if let Err(e) = &result {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
        }
        let http_st = match &result {
            Ok(_) => 200u16,
            Err(e) => crate::audit_emit::http_status_for_error(e),
        };
        let reason = result
            .as_ref()
            .err()
            .map(|e| crate::audit_emit::truncate_reason(&e.to_string()));
        spawn_audit_emit(
            state.audit_client.clone(),
            headers.clone(),
            peer,
            "PATCH",
            path.clone(),
            "accounts.account.update_status",
            org,
            ActorType::Anonymous,
            String::new(),
            vec![],
            "account",
            id,
            http_st,
            reason,
            meta,
        );
    }

    let account = result?;
    Ok(Json(account.into()))
}

pub async fn close_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<Uuid>,
) -> Result<Json<AccountResponse>, AppError> {
    let environment = extract_environment(&headers)?;
    let path = format!("/api/v1/accounts/{id}");
    let result = AccountService::close_account(&state.pool, id, &environment).await;
    let org = match &result {
        Ok(account) => account.organization_id,
        Err(_) => AccountService::get_account(&state.pool, id, &environment)
            .await
            .ok()
            .and_then(|a| a.organization_id),
    };
    if let Some(org) = org {
        let mut meta = HashMap::default();
        if result.is_ok() {
            meta.insert("new_status".into(), "closed".into());
        } else if let Err(e) = &result {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
        }
        let http_st = match &result {
            Ok(_) => 200u16,
            Err(e) => crate::audit_emit::http_status_for_error(e),
        };
        let reason = result
            .as_ref()
            .err()
            .map(|e| crate::audit_emit::truncate_reason(&e.to_string()));
        spawn_audit_emit(
            state.audit_client.clone(),
            headers.clone(),
            peer,
            "DELETE",
            path.clone(),
            "accounts.account.close",
            org,
            ActorType::Anonymous,
            String::new(),
            vec![],
            "account",
            id,
            http_st,
            reason,
            meta,
        );
    }

    let account = result?;
    Ok(Json(account.into()))
}

pub async fn deposit(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<crate::handlers::accounts::DepositRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    let path = format!("/api/v1/accounts/{id}/deposit");

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation(
            "Amount must be greater than zero".to_string(),
        ));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());
    let started_at = Instant::now();
    log_money_request_boundary(
        "START",
        "deposit",
        &path,
        correlation_id.as_deref(),
        &idempotency_key,
        None,
        None,
    );

    let deposit_result = AccountService::deposit_with_idempotency(
        &state.pool,
        id,
        &environment,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id.clone(),
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await;
    let deposit_status = match &deposit_result {
        Ok(_) => 200u16,
        Err(e) => crate::audit_emit::http_status_for_error(e),
    };
    log_money_request_boundary(
        "END",
        "deposit",
        &path,
        correlation_id.as_deref(),
        &idempotency_key,
        Some(deposit_status),
        Some(started_at.elapsed().as_millis() as u64),
    );

    let org = match &deposit_result {
        Ok((account, _)) => account.organization_id,
        Err(_) => AccountService::get_account(&state.pool, id, &environment)
            .await
            .ok()
            .and_then(|a| a.organization_id),
    };
    if let Some(org) = org {
        let mut meta = HashMap::default();
        meta.insert("idempotency_key_present".into(), "true".into());
        if let Err(e) = &deposit_result {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
        }
        let http_st = match &deposit_result {
            Ok(_) => 200u16,
            Err(e) => crate::audit_emit::http_status_for_error(e),
        };
        let reason = deposit_result
            .as_ref()
            .err()
            .map(|e| crate::audit_emit::truncate_reason(&e.to_string()));
        spawn_audit_emit(
            state.audit_client.clone(),
            headers.clone(),
            peer,
            "POST",
            path.clone(),
            "accounts.money.deposit",
            org,
            ActorType::Anonymous,
            String::new(),
            vec![],
            "account",
            id,
            http_st,
            reason,
            meta,
        );
    }

    let (account, transaction) = deposit_result?;

    let mut account_resp = AccountResponse::from(account.clone());
    if let Some(org_id) = account.organization_id {
        let currency = account.currency.as_deref().unwrap_or("USD");
        let started_at = Instant::now();
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        tracing::event!(
            Level::INFO,
            target = "accounts.latency",
            account_id = %id,
            operation = "deposit",
            elapsed_ms,
            "get_account_balance_timing"
        );
        account_resp = account_resp.with_balance(display_balance);
    }

    let txn_resp =
        AccountTransactionResponse::for_mutation_response(&transaction, account_resp.balance);
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<crate::handlers::accounts::WithdrawRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    let path = format!("/api/v1/accounts/{id}/withdraw");

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation(
            "Amount must be greater than zero".to_string(),
        ));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());

    let withdraw_result = AccountService::withdraw_with_idempotency(
        &state.pool,
        id,
        &environment,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id.clone(),
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await;

    let org = match &withdraw_result {
        Ok((account, _)) => account.organization_id,
        Err(_) => AccountService::get_account(&state.pool, id, &environment)
            .await
            .ok()
            .and_then(|a| a.organization_id),
    };
    if let Some(org) = org {
        let mut meta = HashMap::default();
        meta.insert("idempotency_key_present".into(), "true".into());
        if let Err(e) = &withdraw_result {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
        }
        let http_st = match &withdraw_result {
            Ok(_) => 200u16,
            Err(e) => crate::audit_emit::http_status_for_error(e),
        };
        let reason = withdraw_result
            .as_ref()
            .err()
            .map(|e| crate::audit_emit::truncate_reason(&e.to_string()));
        spawn_audit_emit(
            state.audit_client.clone(),
            headers.clone(),
            peer,
            "POST",
            path.clone(),
            "accounts.money.withdraw",
            org,
            ActorType::Anonymous,
            String::new(),
            vec![],
            "account",
            id,
            http_st,
            reason,
            meta,
        );
    }

    let (account, transaction) = withdraw_result?;

    let mut account_resp = AccountResponse::from(account.clone());
    if let Some(org_id) = account.organization_id {
        let currency = account.currency.as_deref().unwrap_or("USD");
        let started_at = Instant::now();
        let display_balance = state
            .ledger_grpc
            .get_account_balance(org_id, &environment, id, currency)
            .await
            .map(|b| negate_ledger_balance_for_display(&b))
            .unwrap_or_else(|e| {
                tracing::warn!(account_id = %id, error = %e, "Ledger balance fetch failed, using 0");
                0
            });
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        tracing::event!(
            Level::INFO,
            target = "accounts.latency",
            account_id = %id,
            operation = "withdraw",
            elapsed_ms,
            "get_account_balance_timing"
        );
        account_resp = account_resp.with_balance(display_balance);
    }

    let txn_resp =
        AccountTransactionResponse::for_mutation_response(&transaction, account_resp.balance);
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<crate::handlers::accounts::TransferRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let environment = extract_environment(&headers)?;
    let path = format!("/api/v1/accounts/{from_id}/transfer");

    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Idempotency-Key header is required".to_string()))?;

    if request.amount <= 0 {
        return Err(AppError::Validation(
            "Amount must be greater than zero".to_string(),
        ));
    }

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty());
    let started_at = Instant::now();
    log_money_request_boundary(
        "START",
        "transfer",
        &path,
        correlation_id.as_deref(),
        &idempotency_key,
        None,
        None,
    );

    let transfer_result = AccountService::transfer_with_idempotency(
        &state.pool,
        from_id,
        &environment,
        request.to_account_id,
        request.amount,
        &idempotency_key,
        &state.ledger_grpc,
        correlation_id.clone(),
        request.description.as_deref(),
        request.external_recipient_id.as_deref(),
        request.reference_id,
    )
    .await;
    let transfer_status = match &transfer_result {
        Ok(_) => 200u16,
        Err(e) => crate::audit_emit::http_status_for_error(e),
    };
    log_money_request_boundary(
        "END",
        "transfer",
        &path,
        correlation_id.as_deref(),
        &idempotency_key,
        Some(transfer_status),
        Some(started_at.elapsed().as_millis() as u64),
    );

    let org = match &transfer_result {
        Ok((from_account, _, _)) => from_account.organization_id,
        Err(_) => AccountService::get_account(&state.pool, from_id, &environment)
            .await
            .ok()
            .and_then(|a| a.organization_id),
    };
    if let Some(org) = org {
        let mut meta = HashMap::default();
        meta.insert("idempotency_key_present".into(), "true".into());
        if let Err(e) = &transfer_result {
            meta.insert(
                "http_status".into(),
                crate::audit_emit::http_status_for_error(e).to_string(),
            );
        }
        let http_st = match &transfer_result {
            Ok(_) => 200u16,
            Err(e) => crate::audit_emit::http_status_for_error(e),
        };
        let reason = transfer_result
            .as_ref()
            .err()
            .map(|e| crate::audit_emit::truncate_reason(&e.to_string()));
        spawn_audit_emit(
            state.audit_client.clone(),
            headers.clone(),
            peer,
            "POST",
            path.clone(),
            "accounts.money.transfer",
            org,
            ActorType::Anonymous,
            String::new(),
            vec![],
            "account",
            from_id,
            http_st,
            reason,
            meta,
        );
    }

    let (from_account, to_account, transaction) = transfer_result?;

    let mut from_resp = AccountResponse::from(from_account.clone());
    let mut to_resp = AccountResponse::from(to_account.clone());

    match (from_account.organization_id, to_account.organization_id) {
        (Some(from_org_id), Some(to_org_id)) if from_org_id == to_org_id => {
            let currency = from_account.currency.as_deref().unwrap_or("USD");
            let started_at = Instant::now();
            let balances_result = state
                .ledger_grpc
                .get_account_balances(
                    from_org_id,
                    &environment,
                    from_account.id,
                    to_account.id,
                    currency,
                )
                .await;
            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            tracing::event!(
                Level::INFO,
                target = "accounts.latency",
                from_account_id = %from_account.id,
                to_account_id = %to_account.id,
                operation = "transfer",
                elapsed_ms,
                "get_account_balances_timing"
            );

            match balances_result {
                Ok((from_balance, to_balance)) => {
                    from_resp =
                        from_resp.with_balance(negate_ledger_balance_for_display(&from_balance));
                    to_resp = to_resp.with_balance(negate_ledger_balance_for_display(&to_balance));
                }
                Err(e) => {
                    tracing::warn!(
                        from_account_id = %from_account.id,
                        to_account_id = %to_account.id,
                        error = %e,
                        "Ledger batched balance fetch failed, using 0"
                    );
                    from_resp = from_resp.with_balance(0);
                    to_resp = to_resp.with_balance(0);
                }
            }
        }
        (Some(_), Some(_)) => {
            tracing::warn!(
                from_account_id = %from_account.id,
                to_account_id = %to_account.id,
                "Transfer accounts in different organizations; using default balances"
            );
        }
        (Some(org_id), None) => {
            let currency = from_account.currency.as_deref().unwrap_or("USD");
            let started_at = Instant::now();
            let display_balance = state
                .ledger_grpc
                .get_account_balance(org_id, &environment, from_account.id, currency)
                .await
                .map(|b| negate_ledger_balance_for_display(&b))
                .unwrap_or_else(|e| {
                    tracing::warn!(account_id = %from_account.id, error = %e, "Ledger balance fetch failed, using 0");
                    0
                });
            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            tracing::event!(
                Level::INFO,
                target = "accounts.latency",
                account_id = %from_account.id,
                operation = "transfer_from_only",
                elapsed_ms,
                "get_account_balance_timing"
            );
            from_resp = from_resp.with_balance(display_balance);
        }
        (None, Some(org_id)) => {
            let currency = to_account.currency.as_deref().unwrap_or("USD");
            let started_at = Instant::now();
            let display_balance = state
                .ledger_grpc
                .get_account_balance(org_id, &environment, to_account.id, currency)
                .await
                .map(|b| negate_ledger_balance_for_display(&b))
                .unwrap_or_else(|e| {
                    tracing::warn!(account_id = %to_account.id, error = %e, "Ledger balance fetch failed, using 0");
                    0
                });
            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            tracing::event!(
                Level::INFO,
                target = "accounts.latency",
                account_id = %to_account.id,
                operation = "transfer_to_only",
                elapsed_ms,
                "get_account_balance_timing"
            );
            to_resp = to_resp.with_balance(display_balance);
        }
        (None, None) => {}
    }

    let txn_resp =
        AccountTransactionResponse::for_mutation_response(&transaction, from_resp.balance);
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
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| D::Error::custom("amount must be a valid integer")),
        serde_json::Value::String(s) => {
            let s = s.trim();
            if let Ok(n) = s.parse::<i64>() {
                Ok(n)
            } else if let Ok(f) = s.parse::<f64>() {
                Ok((f * 100.0).round() as i64)
            } else {
                Err(D::Error::custom(
                    "amount must be a number or numeric string",
                ))
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
        let json =
            r#"{"to_account_id": "550e8400-e29b-41d4-a716-446655440000", "amount": "50.50"}"#;
        let req: TransferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.amount, 5050);
    }
}
