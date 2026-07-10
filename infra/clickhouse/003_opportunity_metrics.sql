-- SPEC-002: Opportunity state-change analytics
CREATE TABLE IF NOT EXISTS opportunity_metrics (
    opportunity_id String,
    from_state String,
    to_state String,
    actor String,
    ts DateTime64(6),
    detail String
) ENGINE = MergeTree()
ORDER BY (opportunity_id, ts);
