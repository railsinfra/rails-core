# Build context: this directory (the crate root).
# Use: `docker build .` here, or Railway with Root Directory = this folder.

FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock build.rs ./
COPY proto ./proto
COPY migrations ./migrations
COPY src ./src

RUN cargo build --locked --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/audit_service /app/audit_service
COPY --from=builder /app/migrations /app/migrations

EXPOSE 8080 50054

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["/app/audit_service"]
