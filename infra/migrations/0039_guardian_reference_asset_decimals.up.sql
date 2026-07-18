-- EP-306 S8 repair: asset precision is Guardian-owned policy input.
-- Existing native assets have canonical 18-decimal precision. Legacy ERC-20
-- rows remain NULL and therefore fail closed until an operator appends a fresh
-- trusted price record carrying the token's verified precision.
ALTER TABLE guardian_reference_prices
    ADD COLUMN asset_decimals SMALLINT
        CHECK (asset_decimals BETWEEN 0 AND 28);

UPDATE guardian_reference_prices
SET asset_decimals = 18
WHERE asset_id LIKE 'eip155:%/native';
