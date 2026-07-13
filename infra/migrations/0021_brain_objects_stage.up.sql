-- EP-201: Add pipeline stage tracking columns for parking/resume
ALTER TABLE brain_objects
    ADD COLUMN current_stage TEXT NOT NULL DEFAULT 'intake',
    ADD COLUMN parked_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_brain_current_stage ON brain_objects(current_stage);
