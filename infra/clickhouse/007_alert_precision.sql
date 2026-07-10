-- SPEC-002: Alert precision daily materialized view
-- Reads from audit_events where action starts with 'alert_'.
-- Populated by EP-203 alert engine; initially empty until alerts flow.
CREATE TABLE IF NOT EXISTS alert_precision_daily (
    date Date,
    alert_kind LowCardinality(String),
    fired UInt32,
    acknowledged UInt32,
    acted_on UInt32,
    precision_pct Float64
) ENGINE = SummingMergeTree()
ORDER BY (date, alert_kind);

CREATE MATERIALIZED VIEW IF NOT EXISTS alert_precision_daily_mv
TO alert_precision_daily
AS SELECT
    toDate(ts) AS date,
    action AS alert_kind,
    countIf(action = 'alert_fired') AS fired,
    countIf(action = 'alert_acknowledged') AS acknowledged,
    countIf(action = 'alert_acted_on') AS acted_on,
    if(fired > 0, acted_on / fired * 100.0, 0.0) AS precision_pct
FROM audit_events
WHERE action LIKE 'alert_%'
GROUP BY date, alert_kind;
