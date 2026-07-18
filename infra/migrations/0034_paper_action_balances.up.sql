-- EP-203/EP-308: explicit paper cash snapshots used by the authoritative
-- router risk check. No balance is fabricated or granted by default.
CREATE TABLE paper_balances (
    actor_id TEXT NOT NULL,
    venue TEXT NOT NULL REFERENCES venues(slug),
    currency TEXT NOT NULL,
    free NUMERIC NOT NULL CHECK (free >= 0),
    locked NUMERIC NOT NULL DEFAULT 0 CHECK (locked >= 0),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (actor_id, venue, currency)
);

