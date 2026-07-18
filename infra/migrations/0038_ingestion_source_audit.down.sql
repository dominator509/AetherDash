DROP TABLE IF EXISTS ingest_source_state;
DROP TABLE IF EXISTS ingest_source_events;
ALTER TABLE brain_objects DROP COLUMN IF EXISTS ladder_rung;
