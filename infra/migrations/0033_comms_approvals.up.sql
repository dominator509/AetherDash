-- EP-308: single-use, action-bound out-of-band approval references.
ALTER TABLE alert_channel_identities
    DROP CONSTRAINT IF EXISTS alert_channel_identities_channel_check;
ALTER TABLE alert_channel_identities
    ADD CONSTRAINT alert_channel_identities_channel_check
    CHECK (channel IN ('telegram', 'discord', 'slack', 'sms', 'email'));

CREATE TABLE approval_references (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    token_hash CHAR(64) UNIQUE NOT NULL,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('execute_paper', 'live_order', 'guardian')),
    target_id TEXT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('telegram', 'discord', 'slack', 'sms', 'email')),
    requires_step_up BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected', 'expired', 'failed')),
    expires_ts TIMESTAMPTZ NOT NULL,
    consumed_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_approval_one_pending
    ON approval_references(actor_id, action, target_id)
    WHERE status = 'pending';
CREATE INDEX idx_approval_expiry ON approval_references(status, expires_ts);

CREATE TABLE approval_attempts (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    approval_id TEXT REFERENCES approval_references(id),
    actor_id TEXT,
    channel TEXT NOT NULL,
    decision TEXT NOT NULL,
    outcome TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_approval_attempts_approval ON approval_attempts(approval_id, created_ts);
