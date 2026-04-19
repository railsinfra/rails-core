-- Enforce single beta application per email (case-insensitive, trimmed).
-- 1. Resolve duplicates: keep earliest by created_at, remove others and log.
-- 2. Normalize existing emails (lowercase + trim).
-- 3. Add unique index on normalized email.

-- Step 1: Remove duplicate rows by normalized email (keep earliest)
DO $$
DECLARE
  deleted_count INTEGER;
  dup_normalized TEXT;
BEGIN
  FOR dup_normalized IN
    SELECT LOWER(TRIM(email)) FROM beta_applications WHERE email IS NOT NULL
    GROUP BY LOWER(TRIM(email))
    HAVING COUNT(*) > 1
  LOOP
    RAISE NOTICE 'beta_email_dedup: Duplicate email - keeping earliest, removing others.';
  END LOOP;

  WITH duplicates_to_remove AS (
    SELECT id FROM (
      SELECT id,
             ROW_NUMBER() OVER (PARTITION BY LOWER(TRIM(email)) ORDER BY created_at ASC, id ASC) AS rn
      FROM beta_applications
      WHERE email IS NOT NULL
    ) t
    WHERE rn > 1
  )
  DELETE FROM beta_applications
  WHERE id IN (SELECT id FROM duplicates_to_remove);

  GET DIAGNOSTICS deleted_count = ROW_COUNT;
  IF deleted_count > 0 THEN
    RAISE NOTICE 'beta_email_dedup: Removed % duplicate row(s).', deleted_count;
  END IF;
END
$$;

-- Step 2: Normalize all existing emails
UPDATE beta_applications
SET email = LOWER(TRIM(email))
WHERE email IS NOT NULL AND (email != LOWER(TRIM(email)));

-- Step 3: Unique index on normalized email (prevents race conditions)
CREATE UNIQUE INDEX IF NOT EXISTS idx_beta_applications_email_normalized
  ON beta_applications (LOWER(TRIM(email)));
