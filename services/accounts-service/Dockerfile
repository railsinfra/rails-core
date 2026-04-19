# Build stage
FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app

ENV SQLX_OFFLINE=true

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY build.rs ./
COPY proto ./proto

# Create dummy src to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    mkdir -p src/config src/errors src/grpc src/handlers src/models src/repositories src/routes src/services src/utils && \
    touch src/config/mod.rs src/errors/mod.rs src/grpc/mod.rs src/handlers/mod.rs src/models/mod.rs src/repositories/mod.rs src/routes/mod.rs src/services/mod.rs src/utils/mod.rs

# Build dependencies only
RUN cargo build --release 2>/dev/null || true

# Remove dummy source
RUN rm -rf src

# Copy actual source code
COPY src ./src
COPY migrations ./migrations
COPY migrations_accounts ./migrations_accounts

# Build the application
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies (curl for HEALTHCHECK)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/accounts-api /app/accounts-api

# Copy migrations for runtime
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/migrations_accounts /app/migrations_accounts

# Expose HTTP and gRPC ports
EXPOSE 8081 9090

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

# Run the binary
CMD ["/app/accounts-api"]
