-- SPEC-011: Revert brain_objects extension
DROP INDEX IF EXISTS idx_brain_raw_sha256;

ALTER TABLE brain_objects
    DROP COLUMN IF EXISTS author_or_publisher,
    DROP COLUMN IF EXISTS published_ts,
    DROP COLUMN IF EXISTS ingested_ts,
    DROP COLUMN IF EXISTS url_or_ref,
    DROP COLUMN IF EXISTS raw_sha256,
    DROP COLUMN IF EXISTS entities,
    DROP COLUMN IF EXISTS linked_events,
    DROP COLUMN IF EXISTS market_keys,
    DROP COLUMN IF EXISTS confidence;
