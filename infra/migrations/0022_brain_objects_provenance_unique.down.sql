-- EP-201: Revert unique index on provenance_hash
DROP INDEX IF EXISTS idx_brain_objects_provenance;
