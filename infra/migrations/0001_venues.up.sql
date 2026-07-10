-- SPEC-002: Venue registry
CREATE TABLE venues (
    slug TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    capabilities JSONB NOT NULL DEFAULT '{}',
    jurisdictions JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    pack_version TEXT NOT NULL DEFAULT '0.1.0',
    created_ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
