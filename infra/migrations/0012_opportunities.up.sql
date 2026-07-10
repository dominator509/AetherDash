-- SPEC-002: Trading opportunities (arbitrage, value, catalyst, hedge)
CREATE TABLE opportunities (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    kind TEXT NOT NULL CHECK (kind IN ('arbitrage','value','catalyst','hedge')),
    legs JSONB NOT NULL DEFAULT '[]',
    gross_edge NUMERIC NOT NULL,
    edge JSONB NOT NULL,
    confidence NUMERIC NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    detected_ts TIMESTAMPTZ NOT NULL,
    expires_ts TIMESTAMPTZ,
    explain_ref JSONB NOT NULL DEFAULT '{}',
    trace_id TEXT NOT NULL CHECK (length(trace_id) = 26),
    state TEXT NOT NULL DEFAULT 'detected' CHECK (state IN ('detected','scored','surfaced','accepted','ignored','expired','executed','closed')),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_opps_state ON opportunities(state);
CREATE INDEX idx_opps_kind ON opportunities(kind);
