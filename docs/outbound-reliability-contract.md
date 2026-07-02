# Outbound Reliability Contract (Rails Core)

This contract defines how any Rails Core service should handle outbound side effects (gRPC/HTTP) so work never hangs indefinitely.

## Goals

- Prevent hidden partial failures where API returns success while dependency write failed.
- Guarantee each outbound task ends in `posted/succeeded`, `pending with retry schedule`, or `failed terminally`.
- Make adoption repeatable for existing and future services.

## Required Lifecycle

1. Validate outbound endpoint config at startup.
2. Persist work intent durably in local DB.
3. Attempt immediate outbound post if applicable.
4. If post fails, schedule retry with bounded backoff.
5. Reconcile stale in-flight records in the background.
6. Expose observable status via API and telemetry.

## Required Data Fields

For durable outbound records (e.g. `transactions`):

- `status` (pending/posting/posted/failed)
- `failure_reason`
- `retry_count`
- `last_attempted_at`
- `next_retry_at`
- `terminal_failure_at`

## Required Startup Safety

- Endpoint URI must be non-empty and parseable.
- Service should fail startup if critical outbound endpoint is malformed.
- Connectivity probes should run before binding listeners for critical dependencies.

## Required Retry Policy

- Exponential backoff with cap.
- Max retry attempts.
- Transition to terminal failed state after exhaustion.
- Never loop forever on unrecoverable errors.

Suggested env knobs:

- `LEDGER_RETRY_MAX_ATTEMPTS`
- `LEDGER_RETRY_BASE_DELAY_MS`
- `LEDGER_RETRY_MAX_DELAY_MS`
- `OUTBOUND_RECONCILE_INTERVAL_SECS`
- `OUTBOUND_MAX_PENDING_AGE_SECS`

## Required Reconciliation

Background sweep must:

- Requeue stale `posting` rows back to `pending`.
- Finalize old `pending` rows to `failed` based on age/attempt policy.
- Emit structured logs for `requeued` and `failed` counts.

## Required API Semantics

- Return `200` only when outbound side effect is confirmed.
- Return `202` when operation is accepted but side effect is deferred/retrying.
- Include transaction/operation identifiers so callers can track state.

### Canonical `202` payload for money mutations

When deposit/withdraw/transfer returns `202`, include these top-level fields:

- `transaction_id` (UUID)
- `status` (`pending` while deferred)
- `retry_count` (integer)
- `next_retry_at` (RFC3339 timestamp or `null`)

This is additive to the existing mutation response body (`account`/`transaction`, or
`from_account`/`to_account`/`transaction`) and gives clients a stable contract for
polling and UX updates.

## Required Tests (TDD)

- Write tests first for each reliability behavior.
- At minimum include:
  - config validation tests
  - retry scheduling tests
  - terminal failure transition tests
  - reconciliation behavior tests
  - HTTP status semantics tests (`200` vs `202`)

## Coverage Policy

- Coverage gate: 100% line + branch for changed modules.
- CI must fail below threshold.

## Accounts-Service Reference Implementation

- Config validation: `services/accounts-service/src/config/settings.rs`
- Startup probe: `services/accounts-service/src/lib.rs`
- Retry metadata + claim logic: `services/accounts-service/src/repositories/transaction_repository.rs`
- Retry worker: `services/accounts-service/src/services/transaction_retry.rs`
- Reconciler: `services/accounts-service/src/services/transaction_reconcile.rs`
- API semantics: `services/accounts-service/src/handlers/accounts.rs`
