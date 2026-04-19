ALTER TABLE users
ADD COLUMN created_by_user_id UUID REFERENCES users(id),
ADD COLUMN created_by_api_key_id UUID REFERENCES api_keys(id);

CREATE INDEX IF NOT EXISTS users_created_by_user_id_idx ON users(created_by_user_id);
CREATE INDEX IF NOT EXISTS users_created_by_api_key_id_idx ON users(created_by_api_key_id);
