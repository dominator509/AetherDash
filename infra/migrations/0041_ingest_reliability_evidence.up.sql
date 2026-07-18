-- EP-206 M4: append-only evidence for source reliability scoring.
CREATE TABLE ingest_source_reliability_evidence (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    source TEXT NOT NULL,
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('correlation','feedback')),
    positive BOOLEAN NOT NULL,
    object_id TEXT REFERENCES brain_objects(id) ON DELETE RESTRICT,
    actor_id TEXT,
    reason TEXT,
    observed_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((evidence_kind='correlation' AND object_id IS NOT NULL AND actor_id IS NULL)
           OR (evidence_kind='feedback' AND actor_id IS NOT NULL)),
    CHECK (observed_ts <= created_ts + INTERVAL '30 seconds')
);
CREATE UNIQUE INDEX uq_ingest_source_correlation_object
    ON ingest_source_reliability_evidence(source,object_id)
    WHERE evidence_kind='correlation';
CREATE INDEX idx_ingest_source_reliability_source_ts
    ON ingest_source_reliability_evidence(source,observed_ts DESC);

CREATE OR REPLACE FUNCTION protect_ingest_reliability_evidence() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'ingest source reliability evidence is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ingest_reliability_evidence_append_only
BEFORE UPDATE OR DELETE ON ingest_source_reliability_evidence
FOR EACH ROW EXECUTE FUNCTION protect_ingest_reliability_evidence();
