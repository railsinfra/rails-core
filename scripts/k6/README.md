# k6 (zero manual API keys by default)

One command after the stack is up:

```bash
make k6-smoke
```

(`make k6-smore` is the same target.)

## What runs

1. **Bootstrap (automatic):** `POST /api/v1/business/register` then `POST /api/v1/api-keys` using the admin JWT from registration. You get a throwaway business, **organization id**, and **API key** with no copy/paste.
2. **Smoke:** `POST /api/v1/users` → `POST /api/v1/accounts` with **`user_id`** → **deposit** (money routes use `Idempotency-Key` and per-VU `X-Forwarded-For`).

`run-smoke.sh` **sources repo `.env`** so optional tokens apply (see below).

## Choose environment: `K6_TARGET` + gateway

| Goal | Command |
|------|---------|
| **Docker** (default gateway `:8080`) | `make k6-smoke` |
| **Staging** | `K6_TARGET=staging K6_GATEWAY_URL=https://your-staging-host make k6-smoke` |
| **Prod** | Same as staging, plus you must allow provisioning: `K6_TARGET=prod K6_GATEWAY_URL=https://… K6_ALLOW_PROVISION_ON_PROD=true make k6-smoke` |

Rules:

- **`K6_GATEWAY_URL`**: scheme + host (and port if needed), **no trailing slash**. Paths are always `${gateway}/users` and `${gateway}/accounts` (same layout as [`gateway/nginx.conf`](../../gateway/nginx.conf)).
- **`K6_TARGET=staging|prod`** without **`K6_GATEWAY_URL`** fails fast with a clear error.

## Internal token (local compose)

If `.env` sets **`INTERNAL_SERVICE_TOKEN_ALLOWLIST`** for users-service, registration requires **`x-internal-service-token`**. Set one matching token in `.env`:

```bash
K6_INTERNAL_SERVICE_TOKEN=the-same-token-listed-in-the-allowlist
```

The smoke script loads `.env` before k6 starts.

## Optional: skip bootstrap (CI with secrets)

```bash
K6_SKIP_BOOTSTRAP=true K6_API_KEY=… K6_ORGANIZATION_ID=… make k6-smoke
```

## Many synthetic IPs (accounts money rate limit)

Money routes use **`ACCOUNTS_TRUSTED_PROXY_IPS`** and **`X-Forwarded-For`** (see [`services/accounts-service/src/routes/rate_limit.rs`](../../services/accounts-service/src/routes/rate_limit.rs)). For heavy runs, raise **`ACCOUNTS_MONEY_RATE_LIMIT_MAX`** in compose / env.

## Files

| File | Role |
|------|------|
| `run-smoke.sh` | `source .env`, default `K6_TARGET=docker`, `k6 run` |
| `config.js` | URLs, prod guard, bootstrap vs skip |
| `lib/bootstrap.js` | Register + mint API key |
| `smoke-user-account-money.js` | `setup()` + smoke scenario |

Planned: burst and interleaved scenarios using the same `buildRuntimeConfig()`.
