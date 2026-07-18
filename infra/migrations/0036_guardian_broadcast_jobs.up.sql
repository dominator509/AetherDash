-- EP-306 M5: restart-safe Guardian custody broadcast and confirmation state.
CREATE TABLE guardian_chain_nonces (
    chain_id BIGINT PRIMARY KEY CHECK (chain_id > 0),
    next_nonce BIGINT NOT NULL CHECK (next_nonce >= 0),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE guardian_broadcast_jobs (
    proposal_id TEXT PRIMARY KEY REFERENCES guardian_proposals(id),
    chain_id BIGINT NOT NULL CHECK (chain_id > 0),
    nonce BIGINT NOT NULL CHECK (nonce >= 0),
    signed_raw TEXT NOT NULL CHECK (signed_raw ~ '^0x02[0-9a-f]+$'),
    tx_hash TEXT NOT NULL CHECK (tx_hash ~ '^0x[0-9a-f]{64}$'),
    state TEXT NOT NULL
        CHECK (state IN ('prepared', 'submitted', 'confirmed', 'failed', 'abandoned')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_attempt_ts TIMESTAMPTZ,
    last_error_code TEXT,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_guardian_broadcast_jobs_due
    ON guardian_broadcast_jobs(state, next_attempt_ts)
    WHERE state IN ('prepared', 'submitted');
CREATE UNIQUE INDEX uq_guardian_active_chain_nonce
    ON guardian_broadcast_jobs(chain_id, nonce)
    WHERE state <> 'abandoned';

CREATE OR REPLACE FUNCTION protect_guardian_broadcast_job() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NOT EXISTS (
            SELECT 1 FROM guardian_proposals p
            WHERE p.id = NEW.proposal_id
              AND p.custody_mode = 'guardian_custody'
              AND p.state IN ('approved', 'auto_approved')
              AND p.approval_expires_at > now()
        ) THEN
            RAISE EXCEPTION 'guardian broadcast job requires a current custody approval';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'guardian broadcast jobs are durable audit records';
    END IF;
    IF NEW.proposal_id IS DISTINCT FROM OLD.proposal_id
       OR NEW.chain_id IS DISTINCT FROM OLD.chain_id
       OR NEW.nonce IS DISTINCT FROM OLD.nonce
       OR NEW.signed_raw IS DISTINCT FROM OLD.signed_raw
       OR NEW.tx_hash IS DISTINCT FROM OLD.tx_hash
       OR NEW.created_ts IS DISTINCT FROM OLD.created_ts THEN
        RAISE EXCEPTION 'guardian broadcast job identity and signed payload are immutable';
    END IF;
    IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
        (OLD.state = 'prepared' AND NEW.state IN ('submitted', 'confirmed', 'failed', 'abandoned'))
        OR (OLD.state = 'submitted' AND NEW.state IN ('confirmed', 'failed'))
    ) THEN
        RAISE EXCEPTION 'invalid guardian broadcast job state transition';
    END IF;
    IF NEW.attempts < OLD.attempts THEN
        RAISE EXCEPTION 'guardian broadcast attempt count cannot move backwards';
    END IF;
    IF NEW.updated_ts < OLD.updated_ts THEN
        RAISE EXCEPTION 'guardian broadcast timestamp cannot move backwards';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER guardian_broadcast_job_immutable
BEFORE INSERT OR UPDATE OR DELETE ON guardian_broadcast_jobs
FOR EACH ROW EXECUTE FUNCTION protect_guardian_broadcast_job();

CREATE OR REPLACE FUNCTION protect_guardian_broadcast_tx_hash() RETURNS trigger AS $$
BEGIN
    IF OLD.tx_hash IS NOT NULL AND NEW.tx_hash IS DISTINCT FROM OLD.tx_hash THEN
        RAISE EXCEPTION 'guardian broadcast transaction hash is immutable once set';
    END IF;
    IF NEW.state IN ('broadcast', 'confirmed', 'failed') THEN
        IF NEW.tx_hash IS NULL OR NEW.tx_hash !~ '^0x[0-9a-f]{64}$' THEN
            RAISE EXCEPTION 'broadcast guardian proposal requires a canonical transaction hash';
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM guardian_broadcast_jobs j
            WHERE j.proposal_id = NEW.id AND j.tx_hash = NEW.tx_hash
              AND j.state IN ('prepared', 'submitted', 'confirmed', 'failed')
        ) THEN
            RAISE EXCEPTION 'guardian proposal transaction hash is not bound to its durable job';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER guardian_broadcast_tx_hash_bound
BEFORE UPDATE ON guardian_proposals
FOR EACH ROW EXECUTE FUNCTION protect_guardian_broadcast_tx_hash();
