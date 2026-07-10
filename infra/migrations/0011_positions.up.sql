-- SPEC-002: Aggregate positions per market (combined live + paper via composite PK)
CREATE TABLE positions (
    market_key TEXT NOT NULL REFERENCES markets(key),
    paper BOOLEAN NOT NULL DEFAULT true,
    side_exposure NUMERIC NOT NULL DEFAULT 0,
    avg_price NUMERIC NOT NULL DEFAULT 0,
    size NUMERIC NOT NULL DEFAULT 0,
    realized_pnl_amount NUMERIC NOT NULL DEFAULT 0,
    realized_pnl_currency TEXT NOT NULL DEFAULT 'USD',
    unrealized_pnl_amount NUMERIC NOT NULL DEFAULT 0,
    unrealized_pnl_currency TEXT NOT NULL DEFAULT 'USD',
    ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (market_key, paper)
);
