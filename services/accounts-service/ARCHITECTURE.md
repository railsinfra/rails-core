# Accounts Microservice Architecture Plan

## Overview
High-performance REST API microservice for managing programmable accounts with support for recurring payments, fixed savings, and account types.

## Performance Requirements
- Process millions of account creations in seconds
- High-throughput transaction processing
- ACID compliance for data integrity

## Technology Stack

### Core Framework
- **Rust** - For high performance and memory safety
- **Actix Web** or **Axum** - Async web framework
- **Tokio** - Async runtime

### Database & ORM
- **SQLx** - Async, compile-time checked SQL with excellent performance
- Native Rust support with zero runtime overhead
- Compile-time query validation
- Excellent async/await support

### Database
- **Neon PostgreSQL** (project: "rails")
- Connection pooling for high concurrency
- ACID transactions

### Additional Libraries
- **serde** - Serialization/deserialization
- **validator** - Input validation
- **chrono** or **time** - Date/time handling
- **uuid** - Unique identifiers
- **dotenv** - Environment configuration
- **tracing** / **log** - Logging
- **anyhow** / **thiserror** - Error handling

## Database Schema Design

### Core Tables

#### 1. `accounts`
```sql
CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_number VARCHAR(50) UNIQUE NOT NULL,
    account_type VARCHAR(20) NOT NULL CHECK (account_type IN ('checking', 'saving')),
    user_id UUID NOT NULL,
    balance DECIMAL(19, 4) NOT NULL DEFAULT 0,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'closed')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    INDEX idx_user_id (user_id),
    INDEX idx_account_number (account_number),
    INDEX idx_status (status)
);
```

#### 2. `recurring_payments`
```sql
CREATE TABLE recurring_payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    recipient_account_id UUID REFERENCES accounts(id) ON DELETE SET NULL,
    external_recipient_id VARCHAR(255), -- For external accounts
    recipient_type VARCHAR(20) NOT NULL CHECK (recipient_type IN ('internal', 'external')),
    amount DECIMAL(19, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    frequency VARCHAR(20) NOT NULL CHECK (frequency IN ('daily', 'weekly', 'biweekly', 'monthly', 'quarterly', 'yearly')),
    trigger_condition JSONB, -- e.g., {"on_salary_received": true, "amount_threshold": 5000}
    next_execution_date DATE NOT NULL,
    last_execution_date DATE,
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'completed', 'cancelled')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    INDEX idx_account_id (account_id),
    INDEX idx_next_execution_date (next_execution_date),
    INDEX idx_status (status)
);
```

#### 3. `fixed_savings_plans`
```sql
CREATE TABLE fixed_savings_plans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    plan_type VARCHAR(20) NOT NULL CHECK (plan_type IN ('auto_withdraw', 'date_locked')),
    initial_amount DECIMAL(19, 4) NOT NULL,
    current_balance DECIMAL(19, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    
    -- For auto_withdraw plans
    monthly_withdraw_amount DECIMAL(19, 4),
    next_withdraw_date DATE,
    
    -- For date_locked plans
    unlock_date DATE,
    
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'cancelled')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    INDEX idx_account_id (account_id),
    INDEX idx_unlock_date (unlock_date),
    INDEX idx_next_withdraw_date (next_withdraw_date),
    INDEX idx_status (status)
);
```

#### 4. `transactions`
```sql
CREATE TABLE transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    transaction_type VARCHAR(20) NOT NULL CHECK (transaction_type IN ('deposit', 'withdrawal', 'transfer', 'recurring_payment', 'savings_withdraw')),
    amount DECIMAL(19, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    balance_after DECIMAL(19, 4) NOT NULL,
    recipient_account_id UUID REFERENCES accounts(id),
    external_recipient_id VARCHAR(255),
    reference_id UUID, -- Links to recurring_payment or fixed_savings_plan
    description TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'completed', 'failed', 'cancelled')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    INDEX idx_account_id (account_id),
    INDEX idx_transaction_type (transaction_type),
    INDEX idx_status (status),
    INDEX idx_created_at (created_at)
);
```

## API Endpoints (RESTful)

### Accounts
- `POST /api/v1/accounts` - Create account
- `GET /api/v1/accounts/{id}` - Get account details
- `GET /api/v1/accounts?user_id={user_id}` - List accounts for user
- `PATCH /api/v1/accounts/{id}` - Update account (limited fields)
- `DELETE /api/v1/accounts/{id}` - Close account

### Recurring Payments
- `POST /api/v1/accounts/{account_id}/recurring-payments` - Create recurring payment
- `GET /api/v1/accounts/{account_id}/recurring-payments` - List recurring payments
- `GET /api/v1/recurring-payments/{id}` - Get recurring payment details
- `PATCH /api/v1/recurring-payments/{id}` - Update recurring payment
- `DELETE /api/v1/recurring-payments/{id}` - Cancel recurring payment

### Fixed Savings Plans
- `POST /api/v1/accounts/{account_id}/fixed-savings` - Create fixed savings plan
- `GET /api/v1/accounts/{account_id}/fixed-savings` - List fixed savings plans
- `GET /api/v1/fixed-savings/{id}` - Get fixed savings plan details
- `PATCH /api/v1/fixed-savings/{id}` - Update fixed savings plan
- `DELETE /api/v1/fixed-savings/{id}` - Cancel fixed savings plan

### Transactions
- `GET /api/v1/accounts/{account_id}/transactions` - List transactions
- `GET /api/v1/transactions/{id}` - Get transaction details

### Health & Metrics
- `GET /health` - Health check
- `GET /metrics` - Prometheus metrics (optional)

## Performance Optimization Strategies

### 1. Database
- Connection pooling (PgBouncer or built-in pool)
- Indexes on frequently queried columns
- Batch inserts for account creation
- Prepared statements
- Read replicas for read-heavy operations

### 2. Application
- Async/await for I/O operations
- Batch processing for bulk account creation
- Caching frequently accessed data (Redis - optional)
- Background job processing for recurring payments
- Rate limiting

### 3. Architecture
- Horizontal scaling capability
- Stateless API design
- Database transactions for ACID compliance
- Idempotent operations

## Background Jobs

### Recurring Payment Processor
- Scheduled job to check and execute recurring payments
- Runs periodically (e.g., every hour or daily)
- Processes payments due on current date
- Updates next_execution_date

### Fixed Savings Processor
- Processes auto-withdraw plans
- Checks date-locked plans for unlock dates
- Updates balances accordingly

## Error Handling

- Comprehensive error types
- Proper HTTP status codes
- Error logging and monitoring
- Transaction rollback on failures

## Security

- Input validation
- SQL injection prevention (parameterized queries)
- Rate limiting
- Authentication/Authorization (to be integrated)
- Audit logging

## Testing Strategy

- Unit tests for business logic
- Integration tests for API endpoints
- Database transaction tests
- Load testing for performance validation

## Deployment Considerations

- Docker containerization
- Environment-based configuration
- Database migrations
- Health checks
- Graceful shutdown
