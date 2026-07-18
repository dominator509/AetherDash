DROP TRIGGER IF EXISTS ingest_reliability_evidence_append_only
    ON ingest_source_reliability_evidence;
DROP FUNCTION IF EXISTS protect_ingest_reliability_evidence();
DROP TABLE IF EXISTS ingest_source_reliability_evidence;
