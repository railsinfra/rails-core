# Build context: this directory (the crate root). Use: `docker build .` from here.
# Railway: set the service root to this folder; Dockerfile path `Dockerfile`.

FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app

# Cargo resolves path dev-dependencies even for `cargo build --release`. The real
# `../audit-service` crate is outside this build context; use a minimal stub so the
# graph resolves (integration tests that need the real crate run only beside this folder).
RUN mkdir -p ../audit-service/src && \
    printf '%s\n' \
      '[package]' \
      'name = "audit_service"' \
      'version = "0.1.0"' \
      'edition = "2021"' \
      '' \
      '[lib]' \
      'path = "src/lib.rs"' \
      > ../audit-service/Cargo.toml && \
    printf '%s\n' '//! Stub for container builds.' > ../audit-service/src/lib.rs

ENV SQLX_OFFLINE=true

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config=1.8.1-1 \
    libssl-dev=3.0.19-1~deb12u2 \
    protobuf-compiler=3.21.12-3 \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY build.rs ./
COPY proto ./proto

RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p src/config src/errors src/grpc src/handlers src/models src/repositories src/routes src/services src/utils && \
    touch src/config/mod.rs src/errors/mod.rs src/grpc/mod.rs src/handlers/mod.rs src/models/mod.rs src/repositories/mod.rs src/routes/mod.rs src/services/mod.rs src/utils/mod.rs

RUN cargo build --release 2>/dev/null || true

RUN rm -rf src

COPY src ./src
COPY migrations ./migrations
COPY migrations_accounts ./migrations_accounts

RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    libssl3=3.0.19-1~deb12u2 \
    curl=7.88.1-10+deb12u14 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/accounts-api /app/accounts-api
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/migrations_accounts /app/migrations_accounts

EXPOSE 8081 9090

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

CMD ["/app/accounts-api"]
