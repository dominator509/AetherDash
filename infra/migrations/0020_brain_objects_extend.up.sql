-- SPEC-011: Extend brain_objects with all required fields
ALTER TABLE brain_objects
    ADD COLUMN author_or_publisher TEXT,
    ADD COLUMN published_ts TIMESTAMPTZ,
    ADD COLUMN ingested_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN url_or_ref TEXT,
    ADD COLUMN raw_sha256 TEXT NOT NULL DEFAULT '',
    ADD COLUMN entities JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN linked_events JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN market_keys JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN confidence NUMERIC DEFAULT NULL CHECK (confidence >= 0 AND confidence <= 1);

CREATE INDEX idx_brain_raw_sha256 ON brain_objects(raw_sha256);
