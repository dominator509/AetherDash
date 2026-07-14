-- SPEC-003: Alert history for the AETHER Alerts service
CREATE TABLE IF NOT EXISTS alert_history (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    rule_name TEXT NOT NULL,
    opportunity_id TEXT NOT NULL,
    channel TEXT NOT NULL,
    summary TEXT NOT NULL,
    net_edge TEXT,
    confidence REAL,
    action TEXT NOT NULL,
    operator_id TEXT,
    status TEXT NOT NULL DEFAULT 'sent',
    message_id TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_alert_history_dedup ON alert_history(opportunity_id, rule_name, channel);
CREATE INDEX idx_alert_history_created ON alert_history(created_ts);
CREATE INDEX idx_alert_history_channel ON alert_history(channel);

CREATE TABLE alert_channel_identities (
    channel TEXT NOT NULL CHECK (channel IN ('telegram', 'discord', 'slack')),
    channel_user_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel, channel_user_id)
);
