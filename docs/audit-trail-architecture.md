# Audit trail architecture (v1)

Central **append-only** audit ingest for Rails-Core. Domain services call **audit-service** over gRPC (`AppendAuditEvent`); rows are stored in a dedicated Postgres database with **no updates or deletes** enforced at the database layer.

## Database

**Manual provisioning (v1):** Neon bootstrap scripts in this repo do **not** create `audit_db`. For local or hosted Postgres:

- Create a database (e.g. `audit_db`) and role with `CREATE` on the schema.
- Set `AUDIT_DATABASE_URL` in `.env` (see repository `.env.example`).
- On first run, **audit-service** applies SQLx migrations (`services/audit-service/migrations/`), including append-only PostgreSQL `RULE`s on `audit_events`.

Indexes (v1): `(organization_id, occurred_at)`, `(correlation_id)`, `(action)`.

## System diagram

```mermaid
flowchart LR
  subgraph clients["HTTP / gateway"]
    GW[nginx :8080]
  end
  subgraph domain["Domain services"]
    US[users-service]
    AC[accounts-service]
    LE[ledger-service]
  end
  AS[audit-service]
  APG[(audit Postgres)]
  US -->|gRPC AppendAuditEvent| AS
  AC -->|gRPC AppendAuditEvent| AS
  LE -->|gRPC AppendAuditEvent| AS
  AS --> APG
  GW --> US
  GW --> AC
  GW --> LE
  GW -->|/audit/health| AS
```

## Sequence: money transfer + ledger post

```mermaid
sequenceDiagram
  participant C as Client
  participant G as gateway
  participant A as accounts-service
  participant L as ledger-service
  participant Q as audit-service
  C->>G: POST /accounts/.../transfer
  G->>A: forward
  A->>A: commit accounts DB
  A->>L: PostTransaction (gRPC)
  L->>L: commit ledger DB
  L->>Q: AppendAuditEvent (ledger.transaction.post)
  A->>Q: AppendAuditEvent (accounts.money.transfer)
  A->>G: 200
  G->>C: 200
```

*(Exact emitter wiring in accounts-service and ledger-service follows the RAI-14 action catalog; ingest and storage are implemented in **audit-service**.)*

## Sequence: login success vs wrong password

```mermaid
sequenceDiagram
  participant C as Client
  participant G as gateway
  participant U as users-service
  participant Q as audit-service
  C->>G: POST /users/api/v1/auth/login
  G->>U: forward
  alt valid credentials
    U->>U: commit session
    U->>Q: AppendAuditEvent (users.auth.login, success)
    U->>G: 200 + tokens
  else wrong password
    U->>Q: AppendAuditEvent (users.auth.login, client_error)
    U->>G: 401
  end
  G->>C: response
```

## Contract

- **Proto:** `proto/audit/v1/audit.proto` — package `rails.core.audit.v1`, unary `AppendAuditEvent` only (no batch in v1).
- **Delivery:** Emitters should call audit **after** the primary domain transaction commits, with a **400 ms** client deadline; failures after a successful domain commit must be visible in logs and Sentry (see RAI-14), without rolling back business data.

## Service layout

| Component        | Path |
|-----------------|------|
| Protobuf        | `proto/audit/v1/audit.proto` |
| Ingest service  | `services/audit-service/` |
| Compose service | `docker-compose.yml` → `audit-service` |
| Gateway (HTTP)  | `gateway/nginx.conf` → `/audit/` → health |

## References

- Linear: RAI-14 (audit trail service), RAI-12 (health parity).
