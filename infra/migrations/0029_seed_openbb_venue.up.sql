-- EP-303 M2: Seed OpenBB venue into the venue registry.
-- SPEC-002: Venue registry schema.
-- Read-only data provider: quotes + reference data via OpenBB platform.

INSERT INTO venues (slug, display_name, capabilities, jurisdictions, enabled, pack_version)
VALUES (
    'openbb',
    'OpenBB',
    '["markets","ticks"]',
    '{"allowed":[],"blocked":[]}',
    false,
    '0.1.0'
)
ON CONFLICT (slug) DO NOTHING;
