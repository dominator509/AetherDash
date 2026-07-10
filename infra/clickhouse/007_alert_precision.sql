-- SPEC-002: Alert precision daily rollup MV (placeholder -- populated by EP-203 alert engine)
CREATE TABLE IF NOT EXISTS alert_precision_daily (
    date Date,
    alert_kind LowCardinality(String),
    fired UInt32,
    acknowledged UInt32,
    acted_on UInt32,
    precision_pct Float64
) ENGINE = SummingMergeTree()
ORDER BY (date, alert_kind);
