-- EP-304: transactional outbox prevents durable fills from being lost on publish failure.
CREATE TABLE paper_fill_outbox (
    fill_id TEXT PRIMARY KEY CHECK (length(fill_id) = 26) REFERENCES fills(id) ON DELETE CASCADE,
    order_id TEXT NOT NULL CHECK (length(order_id) = 26) REFERENCES orders(order_id) ON DELETE CASCADE,
    market_key TEXT NOT NULL REFERENCES markets(key),
    payload JSONB NOT NULL,
    published_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_paper_fill_outbox_pending
    ON paper_fill_outbox(created_ts)
    WHERE published_ts IS NULL;
