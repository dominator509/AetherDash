-- SPEC-002: LLM call audit -- 180d TTL, cost metrics source for OBSERVABILITY.md
CREATE TABLE IF NOT EXISTS llm_calls (
    ts DateTime,
    provider LowCardinality(String),
    model LowCardinality(String),
    purpose LowCardinality(String),
    prompt_tokens UInt32,
    completion_tokens UInt32,
    cached_tokens UInt32 DEFAULT 0,
    cost_usd Decimal64(6),
    cache_hit UInt8 DEFAULT 0,
    latency_ms UInt32,
    trace_id String
) ENGINE = MergeTree()
ORDER BY (provider, model, ts)
TTL ts + INTERVAL 180 DAY;

-- Materialized view: daily cost rollup
CREATE TABLE IF NOT EXISTS llm_cost_daily (
    date Date,
    provider LowCardinality(String),
    model LowCardinality(String),
    purpose LowCardinality(String),
    call_count UInt32,
    total_prompt_tokens UInt64,
    total_completion_tokens UInt64,
    total_cached_tokens UInt64,
    total_cost_usd Decimal64(6),
    cache_hit_ratio Float64
) ENGINE = SummingMergeTree()
ORDER BY (date, provider, model, purpose);

CREATE MATERIALIZED VIEW IF NOT EXISTS llm_cost_daily_mv
TO llm_cost_daily
AS SELECT
    toDate(ts) AS date,
    provider,
    model,
    purpose,
    count() AS call_count,
    sum(prompt_tokens) AS total_prompt_tokens,
    sum(completion_tokens) AS total_completion_tokens,
    sum(cached_tokens) AS total_cached_tokens,
    sum(cost_usd) AS total_cost_usd,
    avg(cache_hit) AS cache_hit_ratio
FROM llm_calls
GROUP BY date, provider, model, purpose;
