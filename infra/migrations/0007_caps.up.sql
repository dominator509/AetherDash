-- SPEC-002: Capabilities version history (versioned JSONB policy documents)
CREATE TABLE caps (
    version SERIAL PRIMARY KEY,
    body JSONB NOT NULL,
    active BOOLEAN NOT NULL DEFAULT false,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
