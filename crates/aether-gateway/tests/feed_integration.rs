#![allow(clippy::expect_used, clippy::unwrap_used)]

use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::time::UtcTime;
use rust_decimal::Decimal;

fn opportunity() -> Opportunity {
    let buy = MarketKey::new(&VenueId::new("kalshi").unwrap(), "feed-test").unwrap();
    let sell = MarketKey::new(&VenueId::new("polymarket").unwrap(), "feed-test").unwrap();
    Opportunity {
        id: Ulid::new(),
        kind: OpportunityKind::Arbitrage,
        legs: vec![
            OpportunityLeg {
                market: buy,
                side: Side::Buy,
                target_price: Some(Decimal::new(4, 1)),
                size_hint: Some(Decimal::ONE),
            },
            OpportunityLeg {
                market: sell,
                side: Side::Sell,
                target_price: Some(Decimal::new(5, 1)),
                size_hint: Some(Decimal::ONE),
            },
        ],
        gross_edge: Decimal::new(1, 1),
        edge: EdgeDecomposition::compute(Decimal::new(1, 1), EdgeCosts::zero()),
        confidence: Confidence::new(Decimal::new(9, 1)).unwrap(),
        detected_ts: UtcTime::now(),
        expires_ts: Some(UtcTime::from_unix_millis(UtcTime::now().unix_millis() + 30_000).unwrap()),
        explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "feed-test".into() },
        trace_id: Ulid::new(),
    }
}

#[tokio::test]
#[ignore = "requires migrated Postgres via DATABASE_URL"]
async fn scored_detection_is_surfaced_once_and_broadcast() {
    let pool =
        sqlx::PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL")).await.unwrap();
    let opportunity = opportunity();
    sqlx::query(
        "INSERT INTO opportunities \
         (id,kind,legs,gross_edge,edge,confidence,detected_ts,expires_ts,explain_ref,trace_id,state) \
         VALUES ($1,'arbitrage',$2,$3,$4,$5,$6::timestamptz,$7::timestamptz,$8,$9,'scored')",
    )
    .bind(opportunity.id.to_string())
    .bind(serde_json::to_value(&opportunity.legs).unwrap())
    .bind(opportunity.gross_edge)
    .bind(serde_json::to_value(&opportunity.edge).unwrap())
    .bind(opportunity.confidence.value())
    .bind(opportunity.detected_ts.to_string())
    .bind(opportunity.expires_ts.unwrap().to_string())
    .bind(serde_json::to_value(&opportunity.explain_ref).unwrap())
    .bind(opportunity.trace_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let (sender, mut receiver) = tokio::sync::broadcast::channel(4);
    assert!(aether_gateway::feed::surface_opportunity(&pool, &sender, &opportunity).await.unwrap());
    let frame = receiver.recv().await.unwrap();
    assert!(frame.contains("\"type\":\"feed_item\""));
    assert!(frame.contains(&opportunity.id.to_string()));
    let state: String = sqlx::query_scalar("SELECT state FROM opportunities WHERE id=$1")
        .bind(opportunity.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "surfaced");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM opportunity_events \
         WHERE opportunity_id=$1 AND from_state='scored' AND to_state='surfaced'",
    )
    .bind(opportunity.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    assert!(aether_gateway::feed::surface_opportunity(&pool, &sender, &opportunity).await.unwrap());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM opportunity_events \
         WHERE opportunity_id=$1 AND from_state='scored' AND to_state='surfaced'",
    )
    .bind(opportunity.id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    sqlx::query("DELETE FROM opportunity_events WHERE opportunity_id=$1")
        .bind(opportunity.id.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM opportunities WHERE id=$1")
        .bind(opportunity.id.to_string())
        .execute(&pool)
        .await
        .unwrap();
}
