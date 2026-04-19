# Project Structure

```
mvp/api/
├── src/
│   ├── main.rs                 # Application entry point, server setup
│   │
│   ├── config/
│   │   ├── mod.rs              # Configuration module
│   │   └── settings.rs         # Application settings from environment
│   │
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── accounts.rs         # Account HTTP handlers
│   │   ├── recurring_payments.rs
│   │   ├── fixed_savings.rs
│   │   └── transactions.rs
│   │
│   ├── models/
│   │   ├── mod.rs
│   │   ├── account.rs           # Account domain models
│   │   ├── recurring_payment.rs
│   │   ├── fixed_savings.rs
│   │   └── transaction.rs
│   │
│   ├── services/
│   │   ├── mod.rs
│   │   ├── account_service.rs  # Business logic for accounts
│   │   ├── recurring_payment_service.rs
│   │   ├── fixed_savings_service.rs
│   │   └── transaction_service.rs
│   │
│   ├── repositories/
│   │   ├── mod.rs
│   │   ├── account_repository.rs # Data access for accounts
│   │   ├── recurring_payment_repository.rs
│   │   ├── fixed_savings_repository.rs
│   │   └── transaction_repository.rs
│   │
│   ├── routes/
│   │   ├── mod.rs
│   │   └── api.rs               # API route definitions
│   │
│   ├── middleware/
│   │   ├── mod.rs
│   │   ├── logging.rs            # Request logging
│   │   └── error_handler.rs     # Global error handling
│   │
│   ├── errors/
│   │   ├── mod.rs
│   │   └── app_error.rs         # Application error types
│   │
│   └── utils/
│       ├── mod.rs
│       └── validation.rs        # Validation utilities
│
├── migrations/
│   └── *.sql                    # SQL migration files
│
├── tests/
│   ├── integration/
│   │   └── accounts_test.rs
│   └── common/
│       └── test_setup.rs
│
├── Cargo.toml                   # Rust dependencies
├── .env.example                 # Environment variables template
├── .gitignore
├── README.md
├── ARCHITECTURE.md
├── IMPLEMENTATION_PLAN.md
├── TECHNICAL_DECISIONS.md
├── SUMMARY.md
└── PROJECT_STRUCTURE.md         # This file
```

## Module Responsibilities

### `config/`
- Load and validate environment variables
- Database connection configuration
- Application settings

### `handlers/`
- HTTP request/response handling
- Request validation
- Response formatting
- Error conversion

### `models/`
- Domain models (structs)
- Request/Response DTOs
- Serialization/Deserialization

### `services/`
- Business logic
- Transaction orchestration
- Validation rules
- Business rule enforcement

### `repositories/`
- Database operations
- SQL queries
- Data mapping
- Transaction management

### `routes/`
- Route definitions
- Route grouping
- Middleware application

### `middleware/`
- Request logging
- Error handling
- Authentication (future)
- Rate limiting (future)

### `errors/`
- Custom error types
- Error conversion
- Error formatting

### `utils/`
- Helper functions
- Validation utilities
- Common operations
