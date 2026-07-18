DROP TRIGGER IF EXISTS trg_apply_opportunity_event ON opportunity_events;
DROP FUNCTION IF EXISTS aether_apply_opportunity_event();
DROP TRIGGER IF EXISTS trg_guard_opportunity_state_update ON opportunities;
DROP FUNCTION IF EXISTS aether_guard_opportunity_state_update();
DROP FUNCTION IF EXISTS aether_opportunity_transition_allowed(TEXT, TEXT);
DROP TABLE IF EXISTS opportunity_detection_outbox;
DROP INDEX IF EXISTS uq_opportunities_open_dedupe;
ALTER TABLE opportunities DROP COLUMN IF EXISTS dedupe_key;
