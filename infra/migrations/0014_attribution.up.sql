-- SPEC-002: Opportunity outcome attribution (predicted vs realized)
CREATE TABLE attribution (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    opportunity_id TEXT NOT NULL CHECK (length(opportunity_id) = 26) REFERENCES opportunities(id),
    predicted_net_edge NUMERIC NOT NULL,
    realized_pnl NUMERIC,
    outcome TEXT,
    closed_ts TIMESTAMPTZ,
    reason_ignored TEXT,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_attr_opp ON attribution(opportunity_id);
