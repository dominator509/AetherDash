-- SPEC-002: Permission grants for human / agent / automation actors
CREATE TABLE permission_grants (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    actor_id TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('human','agent','automation')),
    tier INTEGER NOT NULL CHECK (tier >= 1 AND tier <= 5),
    scopes JSONB NOT NULL DEFAULT '{}',
    expires_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_perm_grants_actor ON permission_grants(actor_id, actor_kind);
