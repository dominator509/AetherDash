-- SPEC-002: Operational metadata key-value store (JSONB values)
CREATE TABLE ops_meta (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL DEFAULT '{}',
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
