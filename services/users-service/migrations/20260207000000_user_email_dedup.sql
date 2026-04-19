-- Enforce single user per email (case-insensitive, trimmed) for beta safety.
-- 1. Resolve duplicates by normalized email: keep earliest record, remove others and log.
-- 2. Normalize all existing emails (lowercase + trim).

-- Step 1: Remove duplicate rows by normalized email (keep earliest by created_at, then id); log conflicts
DO $$
DECLARE
  deleted_count INTEGER;
  dup_normalized TEXT;
BEGIN
  -- Log which normalized emails had duplicates (before we delete)
  FOR dup_normalized IN
    SELECT LOWER(TRIM(email)) AS norm FROM users WHERE email IS NOT NULL
    GROUP BY LOWER(TRIM(email))
    HAVING COUNT(*) > 1
  LOOP
    RAISE NOTICE 'user_email_dedup: Duplicate email "%" - keeping earliest record, removing others.', dup_normalized;
  END LOOP;

  WITH duplicates_to_remove AS (
    SELECT id FROM (
      SELECT id,
             ROW_NUMBER() OVER (PARTITION BY LOWER(TRIM(email)) ORDER BY created_at ASC, id ASC) AS rn
      FROM users
      WHERE email IS NOT NULL
    ) t
    WHERE rn > 1
  )
  DELETE FROM users
  WHERE id IN (SELECT id FROM duplicates_to_remove);

  GET DIAGNOSTICS deleted_count = ROW_COUNT;
  IF deleted_count > 0 THEN
    RAISE NOTICE 'user_email_dedup: Removed % duplicate user row(s).', deleted_count;
  END IF;
END
$$;

-- Step 2: Normalize all existing emails (unique constraint remains satisfied)
UPDATE users
SET email = LOWER(TRIM(email))
WHERE email IS NOT NULL AND (email != LOWER(TRIM(email)));
