# Accounts Microservice

High-performance REST API microservice for managing programmable accounts with support for recurring payments, fixed savings, and multiple account types.

## Features

1. **Account Management**
   - Create checking and saving accounts
   - High-throughput account creation (millions in seconds)
   - ACID-compliant transactions

2. **Recurring Payments**
   - Setup recurring payments to internal or external accounts
   - Trigger-based payments (e.g., on salary receipt)
   - Flexible frequency options (daily, weekly, monthly, etc.)

3. **Fixed Savings Plans**
   - Auto-withdraw plans: Save amount with monthly auto-withdrawal
   - Date-locked plans: Save amount accessible only on specific date

## Technology Stack

- **Rust** - High-performance systems programming
- **Axum** - Modern async web framework
- **SQLx** - Async SQL toolkit with compile-time query checking
- **PostgreSQL** (Neon) - ACID-compliant database

## Project Structure

```
api/
├── src/
│   ├── main.rs                 # Application entry point
│   ├── config/                 # Configuration management
│   ├── handlers/               # HTTP request handlers
│   ├── models/                 # Domain models
│   ├── services/               # Business logic
│   ├── repositories/           # Data access layer
│   ├── routes/                 # API route definitions
│   ├── middleware/             # Custom middleware
│   ├── errors/                 # Error types
│   └── utils/                  # Utility functions
├── prisma/
│   └── schema.prisma           # Database schema
├── migrations/                 # SQL migrations
├── tests/                      # Integration tests
├── Cargo.toml                  # Rust dependencies
├── .env.example                # Environment variables template
└── README.md                   # This file
```

## Setup

### Prerequisites

- Rust (latest stable version)
- PostgreSQL (via Neon)
- Prisma CLI (for schema management)

### Environment Variables

Create a `.env` file:

```env
DATABASE_URL=postgresql://user:password@host:port/rails?sslmode=require
RUST_LOG=info
PORT=8080
```

### Database Setup

1. Install SQLx CLI: `cargo install sqlx-cli`
2. Set up database: `sqlx database create`
3. Run migrations: `sqlx migrate run`

### Running the Service

```bash
cargo run
```

## API Documentation

See `ARCHITECTURE.md` for detailed API endpoint documentation.

## Performance Considerations

- Connection pooling for database
- Batch processing for bulk operations
- Async/await for I/O operations
- Background job processing for recurring payments

## Development

### Running Tests

```bash
cargo test
```

### Code Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

## Database Migrations

This project uses SQLx migrations for database schema management:
- Migrations are located in `migrations/` directory
- Run migrations with: `sqlx migrate run`
- Create new migrations with: `sqlx migrate add <migration_name>`
