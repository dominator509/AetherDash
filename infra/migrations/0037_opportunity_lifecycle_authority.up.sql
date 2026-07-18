-- EP-307: make the accepted SPEC-012 lifecycle and open-chain dedupe durable.
ALTER TABLE opportunities ADD COLUMN dedupe_key TEXT;

CREATE UNIQUE INDEX uq_opportunities_open_dedupe
    ON opportunities(dedupe_key)
    WHERE dedupe_key IS NOT NULL AND state <> 'closed';

CREATE TABLE opportunity_detection_outbox (
    event_id TEXT PRIMARY KEY CHECK (length(event_id) = 26),
    opportunity_id TEXT NOT NULL UNIQUE CHECK (length(opportunity_id) = 26)
        REFERENCES opportunities(id),
    payload JSONB NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_ts TIMESTAMPTZ
);

CREATE INDEX idx_opportunity_detection_outbox_pending
    ON opportunity_detection_outbox(created_ts, event_id)
    WHERE published_ts IS NULL;

CREATE OR REPLACE FUNCTION aether_opportunity_transition_allowed(from_state TEXT, to_state TEXT)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE from_state
        WHEN 'detected' THEN to_state IN ('scored', 'expired')
        WHEN 'scored' THEN to_state IN ('surfaced', 'expired')
        WHEN 'surfaced' THEN to_state IN ('accepted', 'ignored', 'expired')
        WHEN 'accepted' THEN to_state = 'executed'
        WHEN 'ignored' THEN to_state = 'closed'
        WHEN 'expired' THEN to_state = 'closed'
        WHEN 'executed' THEN to_state = 'closed'
        ELSE FALSE
    END
$$;

CREATE OR REPLACE FUNCTION aether_guard_opportunity_state_update()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.state IS DISTINCT FROM OLD.state AND pg_trigger_depth() < 2 THEN
        RAISE EXCEPTION 'opportunity state changes require an opportunity_events row'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_guard_opportunity_state_update
BEFORE UPDATE OF state ON opportunities
FOR EACH ROW EXECUTE FUNCTION aether_guard_opportunity_state_update();

CREATE OR REPLACE FUNCTION aether_apply_opportunity_event()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    current_state TEXT;
BEGIN
    SELECT state INTO current_state
      FROM opportunities
     WHERE id = NEW.opportunity_id
     FOR UPDATE;

    IF current_state IS NULL THEN
        RAISE EXCEPTION 'opportunity % does not exist', NEW.opportunity_id
            USING ERRCODE = '23503';
    END IF;
    IF NEW.from_state IS DISTINCT FROM current_state THEN
        RAISE EXCEPTION 'stale opportunity transition for %: expected %, got %',
            NEW.opportunity_id, current_state, NEW.from_state
            USING ERRCODE = '23514';
    END IF;
    IF NOT aether_opportunity_transition_allowed(NEW.from_state, NEW.to_state) THEN
        RAISE EXCEPTION 'illegal opportunity transition: % -> %', NEW.from_state, NEW.to_state
            USING ERRCODE = '23514';
    END IF;
    IF NEW.to_state = 'closed'
       AND NOT EXISTS (
           SELECT 1 FROM attribution WHERE opportunity_id = NEW.opportunity_id
       ) THEN
        RAISE EXCEPTION 'closed opportunity % requires attribution', NEW.opportunity_id
            USING ERRCODE = '23514';
    END IF;

    UPDATE opportunities
       SET state = NEW.to_state,
           updated_ts = NEW.ts
     WHERE id = NEW.opportunity_id;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_apply_opportunity_event
BEFORE INSERT ON opportunity_events
FOR EACH ROW EXECUTE FUNCTION aether_apply_opportunity_event();
