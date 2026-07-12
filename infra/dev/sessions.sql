-- EP-004 M5: Dev seed data for sessions + grants.
-- Tables created by migrations 0004 (users), 0005 (sessions), 0019 (auth columns), 0006 (permission_grants).
-- Pre-computed SHA-256 hashes of test tokens for hash-based auth.
--
-- Run manually against the dev Postgres AFTER migrations:
--   psql -U aether -d aether -f infra/dev/sessions.sql
-- Or via docker compose exec:
--   docker compose -f infra/dev/docker-compose.yml exec -T postgres \
--     psql -U aether -d aether < infra/dev/sessions.sql

-- ── Users (matching 0004_users) ──────────────────────────────────────

INSERT INTO users (id, display_name) VALUES
    ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'Alice'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FBF', 'Bob'),
    ('01ARZ3NDEKTSV4RRFFQ69G5FCF', 'Admin')
ON CONFLICT (id) DO NOTHING;

-- ── Sessions (matching 0005_sessions + 0019_sessions_add_auth_columns) ──
-- token_hash columns hold hex SHA-256 of the test token.
-- TODO(EP-401): upgrade stored hashes to argon2id.

-- SHAs: echo -n "test-{name}" | sha256sum
-- test-alice  → 321e3403c12a7eabaf0626bda6f5c9bee2b24c6715d3ee3defec577d8adcf176
-- test-bob    → ce5eb0a491d6bd319518fc8b50f7781d6e52677fc78ef56e811e38c9b430a873
-- test-admin  → db09d473d4b6461b91bfa47e4fed3ef55e0234df4132ca7a827b0a69e8927cac
-- test-viewer → bd02452d9f0e44b30a7eda3ca6ff0c87decc20740fd63183b29873d2ee05fef8
-- test-trader → 4e89c1daadf8d55305ec10664c61d9ff9c152f4c61e73ae4b780a6fc403f4eb1
INSERT INTO sessions (id, user_id, token_hash, tier, origin_kind, device_label, expires_ts, last_seen_ts) VALUES
    ('01ARZ3NDEKTSV4RRFFQ69G5FDV', '01ARZ3NDEKTSV4RRFFQ69G5FAV',
     '321e3403c12a7eabaf0626bda6f5c9bee2b24c6715d3ee3defec577d8adcf176',
     1, 'human', 'dev-terminal',
     '2099-12-31 23:59:59+00', now()),
    ('01ARZ3NDEKTSV4RRFFQ69G5FDW', '01ARZ3NDEKTSV4RRFFQ69G5FBF',
     'ce5eb0a491d6bd319518fc8b50f7781d6e52677fc78ef56e811e38c9b430a873',
     3, 'human', 'dev-terminal',
     '2099-12-31 23:59:59+00', now()),
    ('01ARZ3NDEKTSV4RRFFQ69G5FDX', '01ARZ3NDEKTSV4RRFFQ69G5FCF',
     'db09d473d4b6461b91bfa47e4fed3ef55e0234df4132ca7a827b0a69e8927cac',
     5, 'human', 'dev-terminal',
     '2099-12-31 23:59:59+00', now()),
    -- Additional sessions for MCP test tokens
    ('01ARZ3NDEKTSV4RRFFQ69G5FDY', '01ARZ3NDEKTSV4RRFFQ69G5FAV',
     'bd02452d9f0e44b30a7eda3ca6ff0c87decc20740fd63183b29873d2ee05fef8',
     1, 'human', 'mcp-viewer',
     '2099-12-31 23:59:59+00', now()),
    ('01ARZ3NDEKTSV4RRFFQ69G5FDZ', '01ARZ3NDEKTSV4RRFFQ69G5FBF',
     '4e89c1daadf8d55305ec10664c61d9ff9c152f4c61e73ae4b780a6fc403f4eb1',
     3, 'agent', 'mcp-trader',
     '2099-12-31 23:59:59+00', now())
ON CONFLICT (id) DO NOTHING;

-- ── Permission grants (matching 0006_permission_grants) ─────────────

INSERT INTO permission_grants (id, actor_id, actor_kind, tier, scopes) VALUES
    ('01ARZ3NDEKTSV4RRFFQ69G5FGA', '01ARZ3NDEKTSV4RRFFQ69G5FAV', 'human', 1,
     '["brain.search", "markets.query"]'::jsonb),
    ('01ARZ3NDEKTSV4RRFFQ69G5FGB', '01ARZ3NDEKTSV4RRFFQ69G5FBF', 'human', 3,
     '["brain.search", "markets.query", "orders.submit_paper"]'::jsonb),
    ('01ARZ3NDEKTSV4RRFFQ69G5FGC', '01ARZ3NDEKTSV4RRFFQ69G5FCF', 'human', 5,
     '["brain.search", "markets.query", "orders.submit_paper", "orders.submit"]'::jsonb)
ON CONFLICT (id) DO NOTHING;
