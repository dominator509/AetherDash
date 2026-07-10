-- SPEC-002: User registry (identity anchor)
CREATE TABLE users (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    display_name TEXT NOT NULL,
    email TEXT,
    auth_provider TEXT NOT NULL DEFAULT 'local',
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
