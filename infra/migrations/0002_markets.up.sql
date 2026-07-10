-- SPEC-002: Market truth table (all venues, all types)
CREATE TABLE markets (
    key TEXT PRIMARY KEY CHECK (key ~ '^mkt:[a-z0-9]+:.+$'),
    venue TEXT NOT NULL REFERENCES venues(slug),
    kind TEXT NOT NULL CHECK (kind IN ('binary_contract','categorical_contract','scalar_contract','equity','option','perp','spot')),
    title TEXT NOT NULL,
    description_ref TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','halted','closed','resolved')),
    close_ts TIMESTAMPTZ,
    resolve_ts TIMESTAMPTZ,
    outcome TEXT,
    jurisdiction_flags TEXT[] NOT NULL DEFAULT '{}',
    venue_ref JSONB NOT NULL DEFAULT '{}',
    meta JSONB NOT NULL DEFAULT '{}',
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_markets_venue_status ON markets(venue, status);
CREATE INDEX idx_markets_status_close ON markets(status, close_ts);
