-- EP-302: Remove Polymarket venue from the registry.
DELETE FROM venues WHERE slug = 'polymarket';
