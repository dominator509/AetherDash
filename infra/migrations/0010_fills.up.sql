-- SPEC-002: Execution fills (trade history)
CREATE TABLE fills (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    order_id TEXT NOT NULL CHECK (length(order_id) = 26) REFERENCES orders(order_id),
    market TEXT NOT NULL REFERENCES markets(key),
    side TEXT NOT NULL CHECK (side IN ('buy','sell','buy_no','sell_no')),
    price NUMERIC NOT NULL,
    size NUMERIC NOT NULL,
    fee_amount NUMERIC NOT NULL,
    fee_currency TEXT NOT NULL,
    venue_ref JSONB NOT NULL DEFAULT '{}',
    ts TIMESTAMPTZ NOT NULL,
    paper BOOLEAN NOT NULL DEFAULT true,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_fills_order ON fills(order_id);
