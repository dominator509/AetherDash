-- EP-201: Revert pipeline stage tracking columns
DROP INDEX IF EXISTS idx_brain_current_stage;

ALTER TABLE brain_objects
    DROP COLUMN IF EXISTS current_stage,
    DROP COLUMN IF EXISTS parked_reason;
