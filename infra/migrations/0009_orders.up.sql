-- SPEC-002: Placed orders (after cap approval)
CREATE TABLE orders (
    order_id TEXT PRIMARY KEY CHECK (length(order_id) = 26),
    intent_id TEXT NOT NULL REFERENCES order_intents(id),
    market TEXT NOT NULL REFERENCES markets(key),
    side TEXT NOT NULL CHECK (side IN ('buy','sell','buy_no','sell_no')),
    price NUMERIC NOT NULL,
    size NUMERIC NOT NULL,
    fee_amount NUMERIC NOT NULL,
    fee_currency TEXT NOT NULL,
    venue_ref JSONB NOT NULL DEFAULT '{}',
    ts TIMESTAMPTZ NOT NULL,
    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open','partial','filled','cancelled','rejected')),
    paper BOOLEAN NOT NULL DEFAULT true,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_orders_intent ON orders(intent_id);
CREATE INDEX idx_orders_market ON orders(market);
