-- SPEC-002: Full hash-chain audit events -- kept forever (no TTL)
CREATE TABLE IF NOT EXISTS audit_events (
    seq UInt64,
    prev_hash String,
    hash String,
    ts DateTime64(6),
    actor String,
    action LowCardinality(String),
    subject String,
    payload_hash String
) ENGINE = MergeTree()
ORDER BY seq;
