-- EP-303 M2: Remove OpenBB venue from the registry.
DELETE FROM venues WHERE slug = 'openbb';
