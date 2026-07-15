-- EP-401: auth/session hardening, grant revocation, step-up challenges, caps lifecycle.
-- TOTP secret material is deliberately NOT stored in Postgres; users hold only
-- a reference to the operator-controlled credential store.

ALTER TABLE users
    ADD COLUMN password_hash TEXT,
    ADD COLUMN totp_secret_ref TEXT,
    ADD COLUMN failed_login_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_login_count >= 0),
    ADD COLUMN locked_until TIMESTAMPTZ;

ALTER TABLE sessions
    ADD COLUMN idle_expires_ts TIMESTAMPTZ,
    ADD COLUMN revoked_ts TIMESTAMPTZ;

UPDATE sessions
SET idle_expires_ts = LEAST(expires_ts, COALESCE(last_seen_ts, created_ts) + INTERVAL '30 days')
WHERE idle_expires_ts IS NULL;

ALTER TABLE sessions ALTER COLUMN idle_expires_ts SET NOT NULL;
CREATE UNIQUE INDEX uq_sessions_token_hash ON sessions(token_hash);
CREATE INDEX idx_sessions_active
    ON sessions(user_id, idle_expires_ts)
    WHERE revoked_ts IS NULL;

ALTER TABLE permission_grants
    ADD COLUMN revoked_ts TIMESTAMPTZ,
    ADD COLUMN created_by TEXT,
    ADD COLUMN created_by_kind TEXT
        CHECK (created_by_kind IN ('human', 'agent', 'automation'));

CREATE INDEX idx_permission_grants_active
    ON permission_grants(actor_id, actor_kind, expires_ts)
    WHERE revoked_ts IS NULL;

CREATE TABLE step_up_challenges (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    token_hash TEXT NOT NULL UNIQUE,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    expires_ts TIMESTAMPTZ NOT NULL,
    consumed_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_ts <= created_ts + INTERVAL '5 minutes'),
    CHECK (consumed_ts IS NULL OR consumed_ts >= created_ts)
);

CREATE INDEX idx_step_up_actor_active
    ON step_up_challenges(actor_id, action, expires_ts)
    WHERE consumed_ts IS NULL;

ALTER TABLE caps
    ADD COLUMN state TEXT NOT NULL DEFAULT 'draft'
        CHECK (state IN ('draft', 'active', 'superseded')),
    ADD COLUMN drafted_by TEXT,
    ADD COLUMN drafted_by_kind TEXT
        CHECK (drafted_by_kind IN ('human', 'agent', 'automation')),
    ADD COLUMN activated_by TEXT,
    ADD COLUMN activated_ts TIMESTAMPTZ,
    ADD COLUMN parent_version INTEGER REFERENCES caps(version);

UPDATE caps
SET state = CASE WHEN active THEN 'active' ELSE 'superseded' END;

ALTER TABLE caps
    ADD CONSTRAINT caps_state_matches_active
    CHECK (active = (state = 'active'));

CREATE UNIQUE INDEX uq_caps_single_active ON caps((state)) WHERE state = 'active';

CREATE FUNCTION protect_caps_versions() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'caps versions are append-only';
    END IF;
    IF NEW.body IS DISTINCT FROM OLD.body
       OR NEW.version IS DISTINCT FROM OLD.version
       OR NEW.created_ts IS DISTINCT FROM OLD.created_ts THEN
        RAISE EXCEPTION 'caps version payloads are immutable';
    END IF;
    IF OLD.state = 'draft' AND NEW.state NOT IN ('draft', 'active') THEN
        RAISE EXCEPTION 'invalid caps transition from draft';
    END IF;
    IF OLD.state = 'active' AND NEW.state NOT IN ('active', 'superseded') THEN
        RAISE EXCEPTION 'invalid caps transition from active';
    END IF;
    IF OLD.state = 'superseded' AND NEW.state <> 'superseded' THEN
        RAISE EXCEPTION 'superseded caps versions cannot be reactivated';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER caps_versions_append_only
BEFORE UPDATE OR DELETE ON caps
FOR EACH ROW EXECUTE FUNCTION protect_caps_versions();
