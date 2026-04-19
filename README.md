# rails-core

Lean **financial infrastructure**: three services (Rust + Rails), an **nginx** gateway, **proto** contracts, **Docker Compose**, and **`.env.example`**.

## One command

```bash
cp .env.example .env   # set USERS_/ACCOUNTS_/LEDGER_ database URLs
make bootstrap         # verify service directories exist
make dev               # http://localhost:8080 — gateway + all services
```

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
├── .gitignore
└── .env.example
```

## Docs

- [docs/architecture.md](docs/architecture.md) — diagram, boundaries, request flow  
- [docs/quickstart.md](docs/quickstart.md) — clone → env → `make dev`  
- [docs/RAILWAY_DEPLOYMENT.md](docs/RAILWAY_DEPLOYMENT.md) — optional Railway helper for Rust services  

## Service manifest

[`config/services.json`](config/services.json) lists service directory paths for `scripts/bootstrap.sh` and `scripts/verify-layout.sh`.
