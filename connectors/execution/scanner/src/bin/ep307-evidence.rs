//! Read-only verifier for the external 24-hour EP-307 paper-run acceptance artifact.

use serde::Serialize;

#[derive(Serialize)]
struct Evidence {
    window_hours: u32,
    opportunities: i64,
    hourly_buckets: i64,
    open_chains: i64,
    missing_attribution: i64,
    executed_chains: i64,
    passed: bool,
}

#[tokio::main]
async fn main() {
    let evidence = match collect().await {
        Ok(value) => value,
        Err(()) => {
            let value = serde_json::json!({
                "window_hours": 24,
                "passed": false,
                "error": "EP-307 evidence could not be collected"
            });
            let _ = serde_json::to_writer(std::io::stdout().lock(), &value);
            std::process::exit(2);
        }
    };
    let passed = evidence.passed;
    if serde_json::to_writer(std::io::stdout().lock(), &evidence).is_err() {
        std::process::exit(2);
    }
    if !passed {
        std::process::exit(1);
    }
}

async fn collect() -> Result<Evidence, ()> {
    let database_url = std::env::var("DATABASE_URL").map_err(|_| ())?;
    let pool = sqlx::PgPool::connect(&database_url).await.map_err(|_| ())?;
    let row: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "WITH windowed AS ( \
           SELECT id,state,detected_ts FROM opportunities \
           WHERE detected_ts >= now() - interval '24 hours' \
         ) \
         SELECT count(*), \
                count(DISTINCT date_trunc('hour',detected_ts)), \
                count(*) FILTER (WHERE state <> 'closed'), \
                count(*) FILTER (WHERE NOT EXISTS ( \
                    SELECT 1 FROM attribution a WHERE a.opportunity_id=windowed.id \
                )), \
                count(*) FILTER (WHERE EXISTS ( \
                    SELECT 1 FROM opportunity_events e \
                    WHERE e.opportunity_id=windowed.id AND e.to_state='executed' \
                )) \
         FROM windowed",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| ())?;
    let passed = row.0 > 0 && row.1 >= 24 && row.2 == 0 && row.3 == 0 && row.4 > 0;
    Ok(Evidence {
        window_hours: 24,
        opportunities: row.0,
        hourly_buckets: row.1,
        open_chains: row.2,
        missing_attribution: row.3,
        executed_chains: row.4,
        passed,
    })
}
