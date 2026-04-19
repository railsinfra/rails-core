# rails-core

[![Codecov](https://codecov.io/gh/railsinfra/rails-core/graph/badge.svg?branch=main)](https://app.codecov.io/gh/railsinfra/rails-core)

## What this project is / is not

**This project is**

- Financial infrastructure backends (users, accounts, double-entry ledger)
- A multi-service system coordinated through an **nginx** gateway
- A developer-first HTTP + gRPC API platform
- **Local-dev first**: Docker Compose on your machine, external Postgres (for example Neon)

**This project is not**

- A single monolith
- A hosted production SaaS product as shipped from this repository
- A Kubernetes-first or cluster-required stack
- Banking licensing, compliance-as-a-service, or a turnkey regulated product

---

Lean **financial infrastructure**: three services (Rust + Rails), an **nginx** gateway, **proto** contracts, **Docker Compose**, and **`.env.example`**.

## Quick start

### 1. Clone and env file

```bash
git clone https://github.com/railsinfra/rails-core.git
cd rails-core
cp .env.example .env
```

### 2. Database URLs

Set `NEON_API_KEY` in `.env` ([Neon API keys](https://neon.tech/docs/manage/api-keys)).

After provisioning, bootstrap prints a credentials table with **short Neon console links**: it tries **Bitly** when `BITLY_ACCESS_TOKEN` is set (see `.env.example`), otherwise **`is.gd`** via the standard library (no extra Python package). Set `NEON_CONSOLE_NO_PUBLIC_SHORTENER=yes` if you must not send console URLs to a third-party shortener.

### 3. Run

```bash
make dev
```

`make dev` runs `make bootstrap`, then **`docker compose up -d --build`**, then waits until **gateway and service health checks** pass (see `scripts/lib/health_check.py`). The first run can take a long time while Rust and Ruby compile inside the containers; override the wait with `DEV_WAIT_TIMEOUT_SEC` if needed. Follow container output with **`make logs`**.

### When things are up

| What | URL / path |
|------|------------|
| Gateway (redirects to docs) | [http://localhost:8080/](http://localhost:8080/) |
| Gateway liveness JSON | [http://localhost:8080/health](http://localhost:8080/health) |
| Static docs (served by gateway) | [http://localhost:8080/docs/](http://localhost:8080/docs/) |
| Users HTTP API (via gateway) | `http://localhost:8080/users/...` |
| Accounts HTTP API (via gateway) | `http://localhost:8080/accounts/...` |
| Ledger HTTP API (via gateway) | `http://localhost:8080/ledger/...` |

Services speak to each other on the Docker network; you normally **do not** need separate host ports for each service. Everything goes through **:8080**.

### Stop the stack


| Command | What it does |
|--------|----------------|
| `make reset` | `docker compose down` (stops local containers; external DBs unchanged). |
| `make stop` | Same as `make reset`. |
| `make logs` | `docker compose logs -f` (requires `.env`; stack must already be running). |
| `make reset-env`  | Rewrites `USERS_DATABASE_URL`, `ACCOUNTS_DATABASE_URL`, and `LEDGER_DATABASE_URL` in `.env` back to the placeholders from `.env.example`. Does **not** delete data in Neon. |
| `make reset-neon` | Deletes the Neon **project** whose id is in `RAILS_CORE_NEON_PROJECT_ID` in `.env`, clears those database URL lines to placeholders, and strips Neon metadata keys. **Requires** `CONFIRM_PURGE_NEON=yes` in the environment, for example: `CONFIRM_PURGE_NEON=yes make reset-neon`. |


### Optional checks

| Command | Purpose |
|--------|---------|
| `make help` | List targets |
| `make verify` | Assert service directories from `config/services.json` exist |
| `make health` | HTTP smoke checks via the gateway: `/health`, `/users/health`, `/accounts/health`, `/ledger/health`, and `/docs/` |
| `make test` | Gateway health JSON (`/health` + per-service paths: users/accounts `healthy`, ledger `ok`) plus contract flow users → accounts → ledger |

## Three services (short)

| Service | Role |
|--------|------|
| **users-service** (Rust) | Businesses, environments, auth, API keys |
| **accounts-service** (Rust) | Accounts, balances, transfers; talks to users + ledger over gRPC |
| **ledger-service** (Rails) | Double-entry ledger over gRPC (and HTTP under `/ledger/`) |

## Example API flow

**Postman:** import [`postman/rails-core-example-flow.postman_collection.json`](postman/rails-core-example-flow.postman_collection.json), keep `base_url` as `http://localhost:8080` (or change it), then run requests **1 → 5** in order; tests save tokens and account IDs into collection variables.

**curl (equivalent):** all calls below use the gateway. Replace emails if you already registered the same address.

```bash
export BASE=http://localhost:8080

# 1) Register a business (creates admin user + tokens)
REG=$(curl -sS -X POST "$BASE/users/api/v1/business/register" \
  -H "Content-Type: application/json" \
  -d '{"name":"Demo Co","website":"https://example.com","admin_first_name":"Ada","admin_last_name":"Lovelace","admin_email":"ada+demo@example.com","admin_password":"SecurePass123!"}')
echo "$REG" | jq .

export ACCESS=$(echo "$REG" | jq -r .access_token)
export ENV_ID=$(echo "$REG" | jq -r .selected_environment_id)

# 2) Create a server API key (plaintext shown once)
KEY_JSON=$(curl -sS -X POST "$BASE/users/api/v1/api-keys" \
  -H "Authorization: Bearer $ACCESS" \
  -H "X-Environment-Id: $ENV_ID" \
  -H "Content-Type: application/json" \
  -d '{}')
echo "$KEY_JSON" | jq .
export API_KEY=$(echo "$KEY_JSON" | jq -r .key)

# 3) Create two checking accounts (different holder emails, same org via API key)
A=$(curl -sS -X POST "$BASE/accounts/api/v1/accounts" \
  -H "X-API-Key: $API_KEY" \
  -H "X-Environment: sandbox" \
  -H "Content-Type: application/json" \
  -d '{"account_type":"checking","email":"holder-a@example.com","first_name":"A","last_name":"One"}')
B=$(curl -sS -X POST "$BASE/accounts/api/v1/accounts" \
  -H "X-API-Key: $API_KEY" \
  -H "X-Environment: sandbox" \
  -H "Content-Type: application/json" \
  -d '{"account_type":"checking","email":"holder-b@example.com","first_name":"B","last_name":"Two"}')
export FROM_ID=$(echo "$A" | jq -r .id)
export TO_ID=$(echo "$B" | jq -r .id)

# 4) Deposit funds on the first account (amounts are in minor units, e.g. cents)
curl -sS -X POST "$BASE/accounts/api/v1/accounts/$FROM_ID/deposit" \
  -H "X-Environment: sandbox" \
  -H "Idempotency-Key: readme-deposit-1" \
  -H "Content-Type: application/json" \
  -d '{"amount":100000}' | jq .

# 5) Transfer between accounts
curl -sS -X POST "$BASE/accounts/api/v1/accounts/$FROM_ID/transfer" \
  -H "X-Environment: sandbox" \
  -H "Idempotency-Key: readme-transfer-1" \
  -H "Content-Type: application/json" \
  -d "{\"to_account_id\":\"$TO_ID\",\"amount\":5000}" | jq .
```

If `INTERNAL_SERVICE_TOKEN_ALLOWLIST` is set in the users service, sensitive routes also require `X-Internal-Service-Token` (see [services/users-service/README.md](services/users-service/README.md)). The sample `docker-compose.yml` does not set it, so local open registration works out of the box.

## Layout

```
rails-core/
│
├── services/
│   ├── users-service/        # Rust: authentication, users, tenants
│   ├── accounts-service/     # Rust: accounts, balances, transfers
│   └── ledger-service/       # Rails: double-entry accounting system
│
├── gateway/
│   └── nginx.conf            # reverse proxy entrypoint
│
├── proto/
│   ├── users.proto
│   ├── accounts.proto
│   └── ledger.proto
│
├── config/
│   └── services.json         # service paths (bootstrap, verify-layout)
│
├── scripts/
│   ├── bootstrap.sh
│   ├── seed.sh
│   ├── reset.sh              # compose down; --clear-env / --purge-neon (see README)
│   ├── verify-layout.sh
│   ├── health-check.sh
│   ├── deploy-railway.sh     # optional Railway helper (Rust services)
│   └── lib/                  # neon_bootstrap.py, health_check.py, …
│
├── infra/
│   ├── docker/
│   └── postgres/
│
├── docs/
│   ├── architecture.md
│   ├── quickstart.md
│   ├── index.html
│   └── RAILWAY_DEPLOYMENT.md
│
├── docker-compose.yml
├── Makefile
├── README.md
├── CONTRIBUTING.md
├── .gitignore
└── .env.example
```

## Docs and contributing

- [docs/architecture.md](docs/architecture.md) — diagram, boundaries, request flow  
- [docs/quickstart.md](docs/quickstart.md) — clone → env → `make dev`  
- [docs/RAILWAY_DEPLOYMENT.md](docs/RAILWAY_DEPLOYMENT.md) — optional Railway helper for Rust services  
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to change code and open a PR  

## Service manifest

[`config/services.json`](config/services.json) lists service directory paths for `scripts/bootstrap.sh` and `scripts/verify-layout.sh`.
