-- EP-303: Remove Hyperliquid venue from the registry.
DELETE FROM venues WHERE slug = 'hyperliquid';
