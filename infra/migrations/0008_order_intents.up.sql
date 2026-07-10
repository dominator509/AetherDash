-- SPEC-002: Order intents (pre-cap pre-decision records)
CREATE TABLE order_intents (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    market TEXT NOT NULL REFERENCES markets(key),
    side TEXT NOT NULL CHECK (side IN ('buy','sell','buy_no','sell_no')),
    order_type TEXT NOT NULL CHECK (order_type IN ('limit','market')),
    limit_price NUMERIC,
    size NUMERIC NOT NULL,
    size_unit TEXT NOT NULL CHECK (size_unit IN ('contracts','shares','base','quote')),
    tif TEXT NOT NULL CHECK (tif IN ('ioc','gtc','day')),
    paper BOOLEAN NOT NULL DEFAULT true,
    origin JSONB NOT NULL,
    quote_snapshot JSONB NOT NULL DEFAULT '{}',
    caps_version TEXT NOT NULL CHECK (length(caps_version) = 26),
    verdict TEXT CHECK (verdict IN ('allow','deny')),
    verdict_reasons JSONB,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','denied','routed','failed')),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_intents_market ON order_intents(market);
CREATE INDEX idx_intents_state ON order_intents(state);
