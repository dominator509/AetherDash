-- SPEC-002: Ingestion events -- INV-4 audit surface
CREATE TABLE IF NOT EXISTS ingest_events (
    ts DateTime64(6),
    source LowCardinality(String),
    ladder_rung UInt8,
    object_id String,
    bytes UInt32,
    status LowCardinality(String)
) ENGINE = MergeTree()
ORDER BY (source, ts);
