-- Add indexes to optimize slow queries

-- Users table indexes
-- Index for email lookups (unique constraint already creates index, but adding explicit index for clarity)
CREATE INDEX IF NOT EXISTS users_email_idx ON users(email);

-- Index for status filtering
CREATE INDEX IF NOT EXISTS users_status_idx ON users(status);

-- Index for environment_id filtering and joins
CREATE INDEX IF NOT EXISTS users_environment_id_idx ON users(environment_id);

-- Composite index for common query pattern: WHERE id = $1 AND environment_id = $2 AND status = 'active'
CREATE INDEX IF NOT EXISTS users_id_env_status_idx ON users(id, environment_id, status) WHERE status = 'active';

-- Index for email + status queries (used in login)
CREATE INDEX IF NOT EXISTS users_email_status_idx ON users(email, status) WHERE status = 'active';

-- Environments table indexes
-- Index for status filtering
CREATE INDEX IF NOT EXISTS environments_status_idx ON environments(status);

-- Index for business_id filtering
CREATE INDEX IF NOT EXISTS environments_business_id_idx ON environments(business_id);

-- Composite index for common query pattern: WHERE id = $1 AND business_id = $2 AND status = 'active'
CREATE INDEX IF NOT EXISTS environments_id_business_status_idx ON environments(id, business_id, status) WHERE status = 'active';

-- Index for ANY array queries: WHERE id = ANY($1::uuid[]) AND status = 'active'
-- This helps with the login query that uses array syntax
CREATE INDEX IF NOT EXISTS environments_id_status_active_idx ON environments(id, status) WHERE status = 'active';

-- Index for type filtering (used in joins)
CREATE INDEX IF NOT EXISTS environments_type_idx ON environments(type);

-- User sessions table indexes
-- Index for user_id lookups
CREATE INDEX IF NOT EXISTS user_sessions_user_id_idx ON user_sessions(user_id);

-- Index for environment_id lookups
CREATE INDEX IF NOT EXISTS user_sessions_environment_id_idx ON user_sessions(environment_id);

-- Composite index for common query pattern
CREATE INDEX IF NOT EXISTS user_sessions_user_env_idx ON user_sessions(user_id, environment_id);

-- Index for active sessions lookup
CREATE INDEX IF NOT EXISTS user_sessions_status_idx ON user_sessions(status) WHERE status = 'active';

-- Businesses table indexes
-- Index for status filtering
CREATE INDEX IF NOT EXISTS businesses_status_idx ON businesses(status) WHERE status = 'active';
