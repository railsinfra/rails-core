# k6 (zero manual API keys by default)

One command after the stack is up:

```bash
make k6-smoke
```

(`make k6-smore` is the same target.)

**Runner:** if the `k6` binary is not installed, `run-smoke.sh` runs **`grafana/k6`** in Docker. For that path, **`K6_GATEWAY_URL`** defaults to **`http://host.docker.internal:8080`** so the container can reach the compose gateway on the host (requires Docker 20.10+ `host-gateway`). Install the [k6 binary](https://k6.io/docs/get-started/installation/) if you prefer not to use Docker for the runner.

## What runs

1. **Bootstrap (automatic):** `POST /api/v1/business/register` then `POST /api/v1/api-keys` using the admin JWT from registration. You get a throwaway business, **organization id**, and **API key** with no copy/paste.
2. **Smoke:** `POST /api/v1/users` → `POST /api/v1/accounts` with `**user_id`** → **deposit** (money routes use `Idempotency-Key` and per-VU `X-Forwarded-For`).

`run-smoke.sh` **sources repo `.env`** so optional tokens apply (see below).

## Choose environment: `K6_TARGET` + gateway


| Goal                                 | Command                                                                                                                                    |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **Docker** (default gateway `:8080`) | `make k6-smoke`                                                                                                                            |
| **Staging**                          | `K6_TARGET=staging K6_GATEWAY_URL=https://your-staging-host make k6-smoke`                                                                 |
| **Prod**                             | Same as staging, plus you must allow provisioning: `K6_TARGET=prod K6_GATEWAY_URL=https://… K6_ALLOW_PROVISION_ON_PROD=true make k6-smoke` |


Rules:

- `**K6_GATEWAY_URL`**: scheme + host (and port if needed), **no trailing slash**. Paths are always `${gateway}/users` and `${gateway}/accounts` (same layout as `[gateway/nginx.conf](../../gateway/nginx.conf)`).
- `**K6_TARGET=staging|prod`** without `**K6_GATEWAY_URL**` fails fast with a clear error.

## Internal token (local compose)

If `.env` sets `**INTERNAL_SERVICE_TOKEN_ALLOWLIST**` for users-service, registration requires `**x-internal-service-token**`. Set one matching token in `.env`:

```bash
K6_INTERNAL_SERVICE_TOKEN=the-same-token-listed-in-the-allowlist
```

The smoke script loads `.env` before k6 starts.

## Troubleshooting

### `create user 200` fails with HTTP **404** (empty body, `duration_ms: 0` in users-service logs)

The request reached users-service but **no route matched**. With Compose, the service runs `cargo run --release` **once at container start**. If you changed Rust routes after the container started, the running binary can be **stale** until Cargo rebuilds. Fix:

```bash
docker compose restart users-service
```

Wait for `http://localhost:8080/users/health` again (rebuild may take a few minutes on first compile after a change).

### `make k6-smoke` exits **0** even when checks show failures

The smoke script sets a **checks** threshold so a failed scenario exits non-zero. Update k6 if you see old behavior.

## Optional: skip bootstrap (CI with secrets)

```bash
K6_SKIP_BOOTSTRAP=true K6_API_KEY=… K6_ORGANIZATION_ID=… make k6-smoke
```

## Many synthetic IPs (accounts money rate limit)

Money routes use `**ACCOUNTS_TRUSTED_PROXY_IPS**` and `**X-Forwarded-For**` (see `[services/accounts-service/src/routes/rate_limit.rs](../../services/accounts-service/src/routes/rate_limit.rs)`). For heavy runs, raise `**ACCOUNTS_MONEY_RATE_LIMIT_MAX**` in compose / env.

## Files


| File                          | Role                                                |
| ----------------------------- | --------------------------------------------------- |
| `run-smoke.sh`                | `source .env`, default `K6_TARGET=docker`, `k6 run` |
| `config.js`                   | URLs, prod guard, bootstrap vs skip                 |
| `lib/bootstrap.js`            | Register + mint API key                             |
| `smoke-user-account-money.js` | `setup()` + smoke scenario                          |


Planned: burst and interleaved scenarios using the same `buildRuntimeConfig()`.