-- EP-302: Seed Polymarket venue into the venue registry.
-- SPEC-002: Venue registry schema.
-- Read-only pack: no order/balance capabilities.

INSERT INTO venues (slug, display_name, capabilities, jurisdictions, enabled, pack_version)
VALUES (
    'polymarket',
    'Polymarket',
    '["markets","ticks","books"]',
    '{"allowed":[],"blocked":["US"]}',
    false,
    '0.1.0'
)
ON CONFLICT (slug) DO NOTHING;
