-- SPEC-002: Brain knowledge objects (AI-derived state, with full-text search)
CREATE TABLE brain_objects (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    origin TEXT NOT NULL,
    trust NUMERIC NOT NULL DEFAULT 0 CHECK (trust >= 0 AND trust <= 1),
    provenance_hash TEXT NOT NULL,
    minio_raw_ref TEXT,
    minio_clean_ref TEXT,
    summary TEXT,
    staleness_rule TEXT,
    expires_ts TIMESTAMPTZ,
    tier TEXT NOT NULL DEFAULT 'warm' CHECK (tier IN ('hot','warm','cold')),
    fts_vector tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(summary, '') || ' ' || coalesce(kind, ''))) STORED,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_brain_tier ON brain_objects(tier);
CREATE INDEX idx_brain_fts ON brain_objects USING GIN (fts_vector);
