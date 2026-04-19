# rails-core

Lean **financial infrastructure**: three isolated services, **nginx** gateway, **proto v1** contracts, **Docker Compose**, one **`.env`**.

## One command

```bash
cp .env.example .env   # set USERS_/ACCOUNTS_/LEDGER_ database URLs
make bootstrap         # init git submodules for all services
make dev               # http://localhost:8080 — gateway + all services
```

## Layout

```
.
├── services/
│   ├── users-service/      # Rust submodule
│   ├── accounts-service/   # Rust submodule
│   └── ledger-service/     # Rails submodule
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

## Maintainer manifest

[`config/services.json`](config/services.json) lists submodule paths for `scripts/bootstrap.sh` and `scripts/verify-layout.sh`.
