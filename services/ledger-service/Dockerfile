# syntax = docker/dockerfile:1
#
# Build context must be the **repository root** (rails-core), same contract as
# `services/accounts-service/Dockerfile` and `services/users-service/Dockerfile`.
# This allows Railway to clone the full monorepo (empty service root directory) and set:
#   RAILWAY_DOCKERFILE_PATH=services/ledger-service/Dockerfile
# (see https://docs.railway.app/builds/dockerfiles and Deploying a Monorepo).

# Make sure RUBY_VERSION matches the Ruby version in .ruby-version and Gemfile
ARG RUBY_VERSION=3.3.0
FROM ruby:$RUBY_VERSION-slim as base

# Rails app lives here
WORKDIR /rails

ENV RAILS_ENV="production" \
    BUNDLE_DEPLOYMENT="1" \
    BUNDLE_PATH="/usr/local/bundle" \
    BUNDLE_WITHOUT="development"


# Throw-away build stage to reduce size of final image
FROM base as build

# Install packages needed to build gems (bookworm pins; libvips-dev for ruby-vips native build)
RUN apt-get update -qq && \
    apt-get install --no-install-recommends -y \
    build-essential=12.9 \
    git=1:2.39.5-0+deb12u3 \
    libpq-dev=15.16-0+deb12u1 \
    libvips-dev=8.14.1-3+deb12u2 \
    pkg-config=1.8.1-1 \
    && rm -rf /var/lib/apt/lists/*

# Install application gems
COPY services/ledger-service/Gemfile services/ledger-service/Gemfile.lock ./
RUN bundle install && \
    rm -rf ~/.bundle/ "${BUNDLE_PATH}"/ruby/*/cache "${BUNDLE_PATH}"/ruby/*/bundler/gems/*/.git

# Copy application code
COPY services/ledger-service/ .


# Final stage for app image
FROM base

# Install packages needed for deployment (libvips42 satisfies ruby-vips at runtime)
RUN apt-get update -qq && \
    apt-get install --no-install-recommends -y \
    curl=7.88.1-10+deb12u14 \
    libvips42=8.14.1-3+deb12u2 \
    postgresql-client=15+248+deb12u1 \
    && rm -rf /var/lib/apt/lists /var/cache/apt/archives

# Copy built artifacts: gems, application
COPY --from=build /usr/local/bundle /usr/local/bundle
COPY --from=build /rails /rails

# Run and own only the runtime files as a non-root user for security
RUN useradd rails --create-home --shell /bin/bash && \
    mkdir -p db log storage tmp && \
    chown -R rails:rails db log storage tmp
USER rails:rails

# Entrypoint prepares the database.
ENTRYPOINT ["/rails/bin/docker-entrypoint"]

# HTTP (Puma) + gRPC (GRPC_PORT / railway.toml ledger-grpc internalPort)
EXPOSE 3000 9090
CMD ["./bin/rails", "server"]
