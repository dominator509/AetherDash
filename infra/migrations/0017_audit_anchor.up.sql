-- SPEC-002: Merkle-chain audit anchor (sequential hash chain)
CREATE TABLE audit_anchor (
    seq BIGINT PRIMARY KEY,
    hash TEXT NOT NULL,
    anchored_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
