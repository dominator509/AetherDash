DROP TRIGGER IF EXISTS caps_versions_append_only ON caps;
DROP FUNCTION IF EXISTS protect_caps_versions();
DROP INDEX IF EXISTS uq_caps_single_active;
ALTER TABLE caps DROP CONSTRAINT IF EXISTS caps_state_matches_active;

ALTER TABLE caps
    DROP COLUMN IF EXISTS parent_version,
    DROP COLUMN IF EXISTS activated_ts,
    DROP COLUMN IF EXISTS activated_by,
    DROP COLUMN IF EXISTS drafted_by_kind,
    DROP COLUMN IF EXISTS drafted_by,
    DROP COLUMN IF EXISTS state;

DROP INDEX IF EXISTS idx_step_up_actor_active;
DROP TABLE IF EXISTS step_up_challenges;

DROP INDEX IF EXISTS idx_permission_grants_active;
ALTER TABLE permission_grants
    DROP COLUMN IF EXISTS created_by_kind,
    DROP COLUMN IF EXISTS created_by,
    DROP COLUMN IF EXISTS revoked_ts;

DROP INDEX IF EXISTS idx_sessions_active;
DROP INDEX IF EXISTS uq_sessions_token_hash;
ALTER TABLE sessions
    DROP COLUMN IF EXISTS revoked_ts,
    DROP COLUMN IF EXISTS idle_expires_ts;

ALTER TABLE users
    DROP COLUMN IF EXISTS locked_until,
    DROP COLUMN IF EXISTS failed_login_count,
    DROP COLUMN IF EXISTS totp_secret_ref,
    DROP COLUMN IF EXISTS password_hash;
