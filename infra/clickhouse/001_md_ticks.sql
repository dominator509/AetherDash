-- SPEC-002: Market data ticks -- 90d TTL, 1m rollup MV kept indefinitely
CREATE TABLE IF NOT EXISTS md_ticks (
    venue String,
    market_key String,
    ts DateTime64(6),
    bid Nullable(String),
    ask Nullable(String),
    mid Nullable(String),
    last Nullable(String),
    bid_size Nullable(String),
    ask_size Nullable(String),
    seq Nullable(Int64)
) ENGINE = MergeTree()
ORDER BY (venue, market_key, ts)
TTL ts + INTERVAL 90 DAY;

-- Materialized view: 1-minute rollup (avg/OHLC, kept indefinitely)
CREATE TABLE IF NOT EXISTS md_ticks_1m (
    venue String,
    market_key String,
    ts_minute DateTime,
    avg_bid Nullable(Float64),
    avg_ask Nullable(Float64),
    open_bid Nullable(String),
    high_bid Nullable(String),
    low_bid Nullable(String),
    close_bid Nullable(String),
    open_ask Nullable(String),
    high_ask Nullable(String),
    low_ask Nullable(String),
    close_ask Nullable(String),
    tick_count UInt32
) ENGINE = SummingMergeTree()
ORDER BY (venue, market_key, ts_minute);

CREATE MATERIALIZED VIEW IF NOT EXISTS md_ticks_1m_mv
TO md_ticks_1m
AS SELECT
    venue,
    market_key,
    toStartOfMinute(ts) AS ts_minute,
    avgOrNull(toFloat64OrNull(bid)) AS avg_bid,
    avgOrNull(toFloat64OrNull(ask)) AS avg_ask,
    any(bid) AS open_bid,
    maxOrNull(bid) AS high_bid,
    minOrNull(bid) AS low_bid,
    anyLast(bid) AS close_bid,
    any(ask) AS open_ask,
    maxOrNull(ask) AS high_ask,
    minOrNull(ask) AS low_ask,
    anyLast(ask) AS close_ask,
    count() AS tick_count
FROM md_ticks
GROUP BY venue, market_key, ts_minute;
