-- EP-304: one attribution row closes each opportunity and records component divergence.
ALTER TABLE attribution
    ADD COLUMN detail JSONB NOT NULL DEFAULT '{}';

CREATE UNIQUE INDEX uq_attribution_opportunity ON attribution(opportunity_id);
