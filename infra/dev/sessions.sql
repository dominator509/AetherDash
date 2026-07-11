-- EP-004 M5: PostgreSQL session/grants schema for gateway + MCP auth.
-- Run manually against the dev Postgres:
--   psql -U aether -d aether -f infra/dev/sessions.sql
-- Or via docker compose exec:
--   docker compose -f infra/dev/docker-compose.yml exec -T postgres \
--     psql -U aether -d aether < infra/dev/sessions.sql

CREATE TABLE IF NOT EXISTS sessions (
    actor_id TEXT PRIMARY KEY,  -- 26-char Crockford ULID
    tier INTEGER NOT NULL CHECK (tier >= 1 AND tier <= 5),
    origin_kind TEXT NOT NULL DEFAULT 'user',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS grants (
    id SERIAL PRIMARY KEY,
    actor_id TEXT NOT NULL REFERENCES sessions(actor_id),
    capability TEXT NOT NULL,  -- e.g. 'orders.submit', 'orders.submit_paper'
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for fast grant lookups
CREATE INDEX IF NOT EXISTS idx_grants_actor ON grants(actor_id);

-- Test data (dev only)
INSERT INTO sessions (actor_id, tier, origin_kind) VALUES
    ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 1, 'user'),      -- alice, tier-1 viewer
    ('01ARZ3NDEKTSV4RRFFQ69G5FBF', 3, 'user'),      -- bob, tier-3 trader
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 5, 'user')       -- admin, tier-5
ON CONFLICT (actor_id) DO NOTHING;

INSERT INTO grants (actor_id, capability) VALUES
    ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'brain.search'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'markets.query'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FBF', 'brain.search'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FBF', 'markets.query'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FBF', 'orders.submit_paper'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 'brain.search'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 'markets.query'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 'orders.submit_paper'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 'orders.submit')
ON CONFLICT DO NOTHING;
