DROP TRIGGER IF EXISTS guardian_events_append_only ON guardian_proposal_events;
DROP FUNCTION IF EXISTS protect_guardian_events();
DROP TRIGGER IF EXISTS guardian_proposal_payload_immutable ON guardian_proposals;
DROP FUNCTION IF EXISTS protect_guardian_proposal_payload();
DROP INDEX IF EXISTS uq_step_up_guardian_reference;
ALTER TABLE step_up_challenges
    DROP COLUMN IF EXISTS session_id,
    DROP COLUMN IF EXISTS approval_reference_id,
    DROP COLUMN IF EXISTS target_id;
DROP TABLE IF EXISTS guardian_proposal_events;
DROP TABLE IF EXISTS guardian_proposals;
DROP TRIGGER IF EXISTS guardian_reference_prices_append_only ON guardian_reference_prices;
DROP FUNCTION IF EXISTS protect_guardian_reference_prices();
DROP TABLE IF EXISTS guardian_reference_prices;
