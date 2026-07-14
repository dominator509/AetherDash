-- EP-301: Seed Kalshi venue into the venue registry.
-- SPEC-002: Venue registry schema.

INSERT INTO venues (slug, display_name, capabilities, jurisdictions, enabled, pack_version)
VALUES (
    'kalshi',
    'Kalshi',
    '["markets","ticks","books","orders","balances"]',
    '{"allowed":["US"],"blocked":[]}',
    false,
    '0.1.0'
)
ON CONFLICT (slug) DO NOTHING;
