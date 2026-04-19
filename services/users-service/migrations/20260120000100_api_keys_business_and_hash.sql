ALTER TABLE api_keys
ADD COLUMN business_id UUID,
ADD COLUMN created_by_user_id UUID REFERENCES users(id);

-- Existing rows: derive business_id from environment
UPDATE api_keys
SET business_id = e.business_id
FROM environments e
WHERE api_keys.environment_id = e.id AND api_keys.business_id IS NULL;

ALTER TABLE api_keys
ALTER COLUMN environment_id DROP NOT NULL;

ALTER TABLE api_keys
RENAME COLUMN key TO key_hash;

ALTER TABLE api_keys
ALTER COLUMN business_id SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS api_keys_key_hash_unique ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS api_keys_business_id_idx ON api_keys(business_id);
CREATE INDEX IF NOT EXISTS api_keys_environment_id_idx ON api_keys(environment_id);
