# rails-core

Lean **financial infrastructure**: three **vendored** services (Rust + Rails), **nginx** gateway, **proto v1** contracts, **Docker Compose**, one **`.env`**.

## One command

```bash
cp .env.example .env   # set USERS_/ACCOUNTS_/LEDGER_ database URLs
make bootstrap         # verify service directories exist
make dev               # http://localhost:8080 — gateway + all services
```

## Layout

```
.
├── services/
│   ├── users-service/      # Rust (vendored source)
│   ├── accounts-service/   # Rust (vendored source)
│   └── ledger-service/     # Rails (vendored source)
├── gateway/nginx.conf
├── proto/users.proto
├── proto/accounts.proto
├── proto/ledger.proto
├── scripts/bootstrap.sh seed.sh reset.sh
├── infra/docker/ infra/postgres/
├── docs/architecture.md docs/quickstart.md docs/index.html
├── docker-compose.yml
├── Makefile
├── .env.example
└── README.md
```

## Docs

- [docs/architecture.md](docs/architecture.md) — diagram, boundaries, request flow  
- [docs/quickstart.md](docs/quickstart.md) — clone → env → `make dev`  
- [docs/RAILWAY_DEPLOYMENT.md](docs/RAILWAY_DEPLOYMENT.md) — optional Railway helper for Rust services  

## Service manifest

[`config/services.json`](config/services.json) lists service directory paths for `scripts/bootstrap.sh` and `scripts/verify-layout.sh`.
