-- SPEC-002: State machine events for opportunity lifecycle
CREATE TABLE opportunity_events (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    opportunity_id TEXT NOT NULL CHECK (length(opportunity_id) = 26) REFERENCES opportunities(id),
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    actor TEXT NOT NULL,
    ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    detail JSONB NOT NULL DEFAULT '{}',
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_opp_events_opp ON opportunity_events(opportunity_id);
