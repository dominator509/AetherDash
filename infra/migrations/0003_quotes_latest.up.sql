-- SPEC-002: Latest quotes snapshot per market (streaming aggregation target)
CREATE TABLE quotes_latest (
    market_key TEXT PRIMARY KEY REFERENCES markets(key),
    bid NUMERIC,
    ask NUMERIC,
    mid NUMERIC,
    last NUMERIC,
    bid_size NUMERIC,
    ask_size NUMERIC,
    ts TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('stream','poll','snapshot')),
    seq INTEGER,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
