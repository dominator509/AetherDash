-- EP-306: durable Wallet Guardian authority and step-up binding.
CREATE TABLE guardian_reference_prices (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    asset_id TEXT NOT NULL,
    price_usd NUMERIC(38, 18) NOT NULL CHECK (price_usd > 0),
    observed_ts TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (observed_ts <= created_ts + INTERVAL '30 seconds')
);
CREATE INDEX idx_guardian_reference_prices_latest
    ON guardian_reference_prices(asset_id, observed_ts DESC);

CREATE OR REPLACE FUNCTION protect_guardian_reference_prices() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'guardian reference prices are append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER guardian_reference_prices_append_only
BEFORE UPDATE OR DELETE ON guardian_reference_prices
FOR EACH ROW EXECUTE FUNCTION protect_guardian_reference_prices();

CREATE TABLE guardian_proposals (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    proposer_actor_id TEXT NOT NULL,
    proposer_actor_kind TEXT NOT NULL
        CHECK (proposer_actor_kind IN ('human', 'agent', 'automation')),
    grant_id TEXT NOT NULL REFERENCES permission_grants(id),
    tx_spec JSONB NOT NULL,
    custody_mode TEXT NOT NULL
        CHECK (custody_mode IN ('guardian_custody', 'wallet_connect')),
    state TEXT NOT NULL
        CHECK (state IN ('pending', 'approved', 'auto_approved', 'denied',
                         'expired', 'broadcast', 'confirmed', 'failed')),
    policy_trace JSONB NOT NULL,
    proposal_hash CHAR(64) NOT NULL,
    value_delta_usd NUMERIC(38, 18) NOT NULL DEFAULT 0,
    approved_at TIMESTAMPTZ,
    approval_expires_at TIMESTAMPTZ,
    tx_hash TEXT,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_ts TIMESTAMPTZ NOT NULL,
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_ts <= created_ts + INTERVAL '10 minutes'),
    CHECK (approval_expires_at IS NULL OR approved_at IS NOT NULL),
    CHECK (approval_expires_at IS NULL
           OR approval_expires_at <= approved_at + INTERVAL '60 seconds'),
    CHECK (state NOT IN ('pending', 'denied') OR approved_at IS NULL),
    CHECK (state NOT IN ('approved', 'auto_approved', 'broadcast', 'confirmed', 'failed')
           OR approved_at IS NOT NULL)
);
CREATE INDEX idx_guardian_proposal_hash ON guardian_proposals(proposal_hash);
CREATE INDEX idx_guardian_proposals_state_expiry
    ON guardian_proposals(state, expires_ts);

CREATE TABLE guardian_proposal_events (
    id TEXT PRIMARY KEY CHECK (length(id) = 26),
    proposal_id TEXT NOT NULL REFERENCES guardian_proposals(id),
    from_state TEXT,
    to_state TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    grant_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_guardian_events_proposal
    ON guardian_proposal_events(proposal_id, created_ts);

CREATE OR REPLACE FUNCTION protect_guardian_proposal_payload() RETURNS trigger AS $$
BEGIN
    IF NEW.tx_spec IS DISTINCT FROM OLD.tx_spec
       OR NEW.custody_mode IS DISTINCT FROM OLD.custody_mode
       OR NEW.policy_trace IS DISTINCT FROM OLD.policy_trace
       OR NEW.proposal_hash IS DISTINCT FROM OLD.proposal_hash
       OR NEW.value_delta_usd IS DISTINCT FROM OLD.value_delta_usd
       OR NEW.proposer_actor_id IS DISTINCT FROM OLD.proposer_actor_id
       OR NEW.proposer_actor_kind IS DISTINCT FROM OLD.proposer_actor_kind
       OR NEW.grant_id IS DISTINCT FROM OLD.grant_id
       OR NEW.created_ts IS DISTINCT FROM OLD.created_ts
       OR NEW.expires_ts IS DISTINCT FROM OLD.expires_ts THEN
        RAISE EXCEPTION 'guardian proposal payloads are immutable';
    END IF;
    IF OLD.approved_at IS NOT NULL AND NEW.approved_at IS DISTINCT FROM OLD.approved_at THEN
        RAISE EXCEPTION 'guardian approval timestamp is immutable once set';
    END IF;
    IF OLD.approval_expires_at IS NOT NULL
       AND NEW.approval_expires_at IS DISTINCT FROM OLD.approval_expires_at THEN
        RAISE EXCEPTION 'guardian approval expiry is immutable once set';
    END IF;
    IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
        (OLD.state = 'pending' AND NEW.state IN ('approved', 'denied', 'expired'))
        OR (OLD.state IN ('approved', 'auto_approved') AND NEW.state IN ('broadcast', 'expired'))
        OR (OLD.state = 'broadcast' AND NEW.state IN ('confirmed', 'failed'))
    ) THEN
        RAISE EXCEPTION 'invalid guardian proposal state transition';
    END IF;
    IF NEW.updated_ts < OLD.updated_ts THEN
        RAISE EXCEPTION 'guardian proposal timestamp cannot move backwards';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER guardian_proposal_payload_immutable
BEFORE UPDATE ON guardian_proposals
FOR EACH ROW EXECUTE FUNCTION protect_guardian_proposal_payload();

ALTER TABLE step_up_challenges
    ADD COLUMN target_id TEXT,
    ADD COLUMN approval_reference_id TEXT REFERENCES approval_references(id),
    ADD COLUMN session_id TEXT REFERENCES sessions(id);

CREATE UNIQUE INDEX uq_step_up_guardian_reference
    ON step_up_challenges(approval_reference_id)
    WHERE approval_reference_id IS NOT NULL;

CREATE OR REPLACE FUNCTION protect_guardian_events() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'guardian proposal events are append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER guardian_events_append_only
BEFORE UPDATE OR DELETE ON guardian_proposal_events
FOR EACH ROW EXECUTE FUNCTION protect_guardian_events();
