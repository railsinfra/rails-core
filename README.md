# rails-core

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

## Quick start (one command after env)

```bash
git clone https://github.com/railsinfra/rails-core.git
cd rails-core
cp .env.example .env
# Edit .env: set USERS_DATABASE_URL, ACCOUNTS_DATABASE_URL, LEDGER_DATABASE_URL (three databases).
make dev
```

`make dev` runs a fast layout check (`make bootstrap`), then starts **Docker Compose**. The first run can take several minutes while Rust and Ruby dependencies compile inside the containers.

### When things are up

| What | URL / path |
|------|------------|
| Gateway (redirects to docs) | [http://localhost:8080/](http://localhost:8080/) |
| Static docs (served by gateway) | [http://localhost:8080/docs/](http://localhost:8080/docs/) |
| Users HTTP API (via gateway) | `http://localhost:8080/users/...` |
| Accounts HTTP API (via gateway) | `http://localhost:8080/accounts/...` |
| Ledger HTTP API (via gateway) | `http://localhost:8080/ledger/...` |

Services speak to each other on the Docker network; you normally **do not** need separate host ports for each service. Everything goes through **:8080**.

### Stop the stack

```bash
make reset
```

### Optional checks

| Command | Purpose |
|--------|---------|
| `make help` | List targets |
| `make verify` | Assert service directories from `config/services.json` exist |
| `make health` | HTTP smoke checks via the gateway (expects the stack to be running) |

## Three services (short)

| Service | Role |
|--------|------|
| **users-service** (Rust) | Businesses, environments, auth, API keys |
| **accounts-service** (Rust) | Accounts, balances, transfers; talks to users + ledger over gRPC |
| **ledger-service** (Rails) | Double-entry ledger over gRPC (and HTTP under `/ledger/`) |

## Example API flow (curl)

All calls below use the gateway. Replace emails if you already registered the same address.

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
│   ├── reset.sh
│   ├── verify-layout.sh
│   ├── health-check.sh
│   ├── deploy-railway.sh     # optional Railway helper (Rust services)
│   └── lib/                  # read_manifest.py, health_check.py
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
