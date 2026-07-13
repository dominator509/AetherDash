-- EP-201: Unique index on provenance_hash for atomic deduplication
CREATE UNIQUE INDEX IF NOT EXISTS idx_brain_objects_provenance ON brain_objects(provenance_hash);
