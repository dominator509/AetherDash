-- EP-206: durable compliance-ladder identity and scheduler recovery state.
ALTER TABLE brain_objects
    ADD COLUMN ladder_rung SMALLINT NOT NULL DEFAULT 6
        CHECK (ladder_rung BETWEEN 1 AND 6);

CREATE TABLE ingest_source_events (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    object_id TEXT NOT NULL UNIQUE REFERENCES brain_objects(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    ladder_rung SMALLINT NOT NULL CHECK (ladder_rung BETWEEN 1 AND 6),
    bytes BIGINT NOT NULL CHECK (bytes >= 0),
    status TEXT NOT NULL CHECK (status IN ('ingested','deduplicated')),
    trace_id TEXT NOT NULL CHECK (length(trace_id) = 26),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ingest_source_events_source_ts
    ON ingest_source_events(source, created_ts DESC);

CREATE TABLE ingest_source_state (
    source TEXT PRIMARY KEY,
    ladder_rung SMALLINT NOT NULL CHECK (ladder_rung BETWEEN 1 AND 6),
    cursor TEXT,
    health TEXT NOT NULL DEFAULT 'unknown'
        CHECK (health IN ('unknown','healthy','degraded','disabled')),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    last_success_ts TIMESTAMPTZ,
    last_error_code TEXT,
    next_run_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ingest_source_state_due
    ON ingest_source_state(next_run_ts) WHERE health <> 'disabled';
