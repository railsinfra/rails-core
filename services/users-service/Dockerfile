# Build context: this directory (the crate root).
# Use: `docker build .` here, or Railway with Root Directory = this folder.

FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app

# Path dev-dependency `../audit-service` must exist for Cargo metadata in this crate-only context.
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
    mkdir -p src/routes && \
    touch src/routes/mod.rs

RUN cargo build --release 2>/dev/null || true

RUN rm -rf src

COPY src ./src
COPY migrations ./migrations

RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    libssl3=3.0.19-1~deb12u2 \
    curl=7.88.1-10+deb12u14 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/users_service /app/users_service
COPY --from=builder /app/migrations /app/migrations

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["/app/users_service"]
