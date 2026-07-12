-- EP-004: Add auth columns to sessions (tier, origin_kind, device_label, last_seen_ts)
ALTER TABLE sessions ADD COLUMN tier INTEGER NOT NULL DEFAULT 1 CHECK (tier >= 1 AND tier <= 5);
ALTER TABLE sessions ADD COLUMN origin_kind TEXT NOT NULL DEFAULT 'human' CHECK (origin_kind IN ('human','agent','automation'));
ALTER TABLE sessions ADD COLUMN device_label TEXT;
ALTER TABLE sessions ADD COLUMN last_seen_ts TIMESTAMPTZ;
