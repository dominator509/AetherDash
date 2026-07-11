-- SPEC-002: Alert precision daily materialized view
-- Aggregates alert events from audit_events where action matches alert_*.
-- Uses MergeTree (not SummingMergeTree) — ClickHouse does not allow
-- SummingMergeTree directly in MATERIALIZED VIEW definitions.
-- Stores additive counts; precision_pct is computed at query time.
-- Populated by EP-203 alert engine; initially empty until alerts flow.
CREATE TABLE IF NOT EXISTS alert_precision_daily (
    date Date,
    alert_kind LowCardinality(String),
    fired UInt32,
    acknowledged UInt32,
    acted_on UInt32
) ENGINE = SummingMergeTree()
ORDER BY (date, alert_kind);

CREATE MATERIALIZED VIEW IF NOT EXISTS alert_precision_daily_mv
TO alert_precision_daily
AS SELECT
    toDate(ts) AS date,
    replaceRegexpOne(action, '_(fired|acknowledged|acted_on)$', '') AS alert_kind,
    countIf(action LIKE '%_fired') AS fired,
    countIf(action LIKE '%_acknowledged') AS acknowledged,
    countIf(action LIKE '%_acted_on') AS acted_on
FROM audit_events
WHERE action LIKE 'alert_%'
GROUP BY date, alert_kind;
