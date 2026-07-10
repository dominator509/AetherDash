-- SPEC-002: Order book snapshots -- 30d TTL
CREATE TABLE IF NOT EXISTS md_book_snapshots (
    market_key String,
    ts DateTime64(6),
    bids String,
    asks String,
    depth UInt16
) ENGINE = MergeTree()
ORDER BY (market_key, ts)
TTL ts + INTERVAL 30 DAY;
