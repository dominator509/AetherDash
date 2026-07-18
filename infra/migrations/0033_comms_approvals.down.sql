DROP TABLE IF EXISTS approval_attempts;
DROP TABLE IF EXISTS approval_references;
ALTER TABLE alert_channel_identities
    DROP CONSTRAINT IF EXISTS alert_channel_identities_channel_check;
ALTER TABLE alert_channel_identities
    ADD CONSTRAINT alert_channel_identities_channel_check
    CHECK (channel IN ('telegram', 'discord', 'slack'));
