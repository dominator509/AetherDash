-- EP-201: make content deduplication atomic within a source.
CREATE UNIQUE INDEX IF NOT EXISTS idx_brain_objects_source_raw_sha256
    ON brain_objects(source, raw_sha256)
    WHERE raw_sha256 <> '';
