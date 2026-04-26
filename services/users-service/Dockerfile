# Build context must be the **repository root** (see `.github/workflows/ci.yml` docker-builds).
# The crate depends on `../audit-service` and `build.rs` compiles protos under `../../proto`.

# Build stage
FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app

ENV SQLX_OFFLINE=true

# Install build dependencies (pinned + --no-install-recommends for reproducible, smaller images)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config=1.8.1-1 \
    libssl-dev=3.0.19-1~deb12u2 \
    protobuf-compiler=3.21.12-3 \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY services/users-service/Cargo.toml services/users-service/Cargo.lock ./
COPY services/users-service/.sqlx ./.sqlx
COPY services/users-service/build.rs ./
COPY services/users-service/proto ./proto
COPY services/audit-service /audit-service
COPY proto /proto

# Create dummy src to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p src/routes && \
    touch src/routes/mod.rs

# Build dependencies only
RUN cargo build --release 2>/dev/null || true

# Remove dummy source
RUN rm -rf src

# Copy actual source code
COPY services/users-service/src ./src
COPY services/users-service/migrations ./migrations

# Build the application
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20230311+deb12u1 \
    libssl3=3.0.19-1~deb12u2 \
    curl=7.88.1-10+deb12u14 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/users_service /app/users_service

# Copy migrations for runtime
COPY --from=builder /app/migrations /app/migrations

# Expose HTTP port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the binary
CMD ["/app/users_service"]
