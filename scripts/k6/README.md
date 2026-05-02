# k6 load tests (compose-first, env-configurable)

Scripts hit **HTTP** the same way clients do. Defaults assume **Docker Compose** with the repo **gateway** on `http://127.0.0.1:8080` and nginx path prefixes from [`gateway/nginx.conf`](../../gateway/nginx.conf):

| Service  | Base URL default              | Upstream path prefix |
|----------|-------------------------------|------------------------|
| users    | `http://127.0.0.1:8080/users` | `/api/v1/...`        |
| accounts | `http://127.0.0.1:8080/accounts` | `/api/v1/...`     |

## Prerequisites

1. Stack up (`make dev` or `docker compose up`) and migrations healthy.
2. A **sandbox API key** for your business (`K6_API_KEY`).
3. **`K6_ORGANIZATION_ID`**: use your **business / organization UUID** (same value you use for accounts `organization_id` in sandbox). From business registration or your dashboard.
4. Install [k6](https://k6.io/docs/get-started/installation/).

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `K6_API_KEY` | yes | — | Plain API key (`X-API-Key`) |
| `K6_ORGANIZATION_ID` | recommended | empty | Passed as `organization_id` on account create |
| `K6_USERS_BASE_URL` | no | `http://127.0.0.1:8080/users` | Override for staging/prod gateway or BFF |
| `K6_ACCOUNTS_BASE_URL` | no | `http://127.0.0.1:8080/accounts` | Override for staging/prod |
| `K6_ENVIRONMENT` | no | `sandbox` | `X-Environment`: `sandbox` or `production` |
| `K6_USER_PASSWORD` | no | `password123!` | Password for each synthetic SDK user |
| `K6_SYNTHETIC_IP_PREFIX` | no | `203.0.113` | First octets for `X-Forwarded-For` (TEST-NET-3 style) |

**Switching to staging or production:** set `K6_USERS_BASE_URL` and `K6_ACCOUNTS_BASE_URL` to the HTTPS bases your edge exposes (must still reach `/api/v1/users` and `/api/v1/accounts` with the same header semantics). Use a **non-production key** until you intentionally load prod.

## Account ↔ user mapping

Flows use **`POST /api/v1/users`** (users-service) then **`POST /api/v1/accounts`** with **`user_id`** set to the returned user (legacy accounts path). This keeps **every account row tied to a real users-service user**.

## Many synthetic client IPs (accounts money rate limit)

Money routes rate-limit by client key from [`ACCOUNTS_TRUSTED_PROXY_IPS`](../../services/accounts-service/src/routes/rate_limit.rs). For k6 to appear as **many IPs**, the **peer** seen by accounts-service must be a **trusted proxy**, and k6 must send **`X-Forwarded-For`** with distinct addresses (the scripts already set per-VU values). In compose, set `ACCOUNTS_TRUSTED_PROXY_IPS` on **accounts-service** to the **gateway/nginx container IP** (or documented bridge IP) that matches the hop in front of accounts. Tune **`ACCOUNTS_MONEY_RATE_LIMIT_MAX`** / window for load experiments.

## Scripts

| File | Purpose |
|------|---------|
| `smoke-user-account-money.js` | 1 VU: create user → account → small deposit |

Planned (same env model): burst-create users then mixed money traffic; interleaved create+transact. Extend under this directory.

## Makefile

```bash
export K6_API_KEY=... K6_ORGANIZATION_ID=...
make k6-smoke
```
