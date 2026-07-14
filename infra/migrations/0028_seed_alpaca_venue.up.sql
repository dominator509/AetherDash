-- EP-303 M4: Seed Alpaca venue into the venue registry.
-- SPEC-002: Venue registry schema.
-- Paper trading pack: orders/balances on paper endpoint only.

INSERT INTO venues (slug, display_name, capabilities, jurisdictions, enabled, pack_version)
VALUES (
    'alpaca',
    'Alpaca',
    '["markets","ticks","orders","balances"]',
    '{"allowed":["US"],"blocked":[]}',
    false,
    '0.1.0'
)
ON CONFLICT (slug) DO NOTHING;
