-- EP-206 M2: durable compliance downgrade ledger and manual-review queue.
CREATE TABLE ingest_rung_decisions (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    source TEXT NOT NULL,
    from_rung SMALLINT NOT NULL CHECK (from_rung BETWEEN 1 AND 5),
    to_rung SMALLINT NOT NULL CHECK (to_rung BETWEEN 2 AND 6),
    reason TEXT NOT NULL CHECK (length(reason) >= 8),
    approved_by TEXT NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (to_rung > from_rung)
);
CREATE INDEX idx_ingest_rung_decisions_source_ts
    ON ingest_rung_decisions(source, created_ts DESC);

CREATE TABLE ingest_manual_review_queue (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    content TEXT NOT NULL,
    raw_content BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','approved','rejected')),
    submitted_by TEXT NOT NULL,
    reviewed_by TEXT,
    reviewed_ts TIMESTAMPTZ,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'pending' AND reviewed_by IS NULL AND reviewed_ts IS NULL)
           OR (status IN ('approved','rejected')
               AND reviewed_by IS NOT NULL AND reviewed_ts IS NOT NULL))
);
CREATE INDEX idx_ingest_manual_review_approved
    ON ingest_manual_review_queue(id) WHERE status = 'approved';
