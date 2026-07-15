#![allow(clippy::unwrap_used)]

use aether_bus::producer::StubProducer;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::opportunity::{EdgeCosts, EdgeDecomposition};
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_paper_ledger::{
    AttributionClose, AttributionComponents, PaperExecutionService, PaperLedger,
    PostgresLedgerStore,
};
use rust_decimal::Decimal;

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn paper_rows_lifecycle_and_attribution_are_transactional_and_segregated() {
    assert_eq!(std::env::var("AETHER_INTEGRATION_TEST").as_deref(), Ok("1"));
    let database_url = std::env::var("DATABASE_URL").unwrap();
    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let store = PostgresLedgerStore::new(pool.clone());
    let suffix = Ulid::new().to_string().to_lowercase();
    let venue_slug = format!("ep304{}", &suffix[..8]);
    let venue = VenueId::new(&venue_slug).unwrap();
    let market = MarketKey::new(&venue, "PAPER").unwrap();
    let opportunity_id = Ulid::new();

    sqlx::query(
        "INSERT INTO venues (slug,display_name,capabilities,jurisdictions,pack_version) \
         VALUES ($1,'EP-304 test','{}','{}','test')",
    )
    .bind(&venue_slug)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO markets (key,venue,kind,title,status,venue_ref,meta) \
         VALUES ($1,$2,'spot','EP-304 paper market','open','{}','{}')",
    )
    .bind(market.as_str())
    .bind(&venue_slug)
    .execute(&pool)
    .await
    .unwrap();
    let edge = EdgeDecomposition::compute(Decimal::new(10, 2), EdgeCosts::zero());
    sqlx::query(
        "INSERT INTO opportunities (id,kind,legs,gross_edge,edge,confidence,detected_ts,explain_ref,trace_id,state) \
         VALUES ($1,'arbitrage','[]',$2,$3,'0.8',now(),'{}',$4,'accepted')",
    )
    .bind(opportunity_id.to_string())
    .bind(edge.net_edge())
    .bind(serde_json::to_value(&edge).unwrap())
    .bind(Ulid::new().to_string())
    .execute(&pool)
    .await
    .unwrap();

    let book = make_book(market.clone());
    let intent = make_intent(market.clone());
    let order_id = intent.id;
    let mut service = PaperExecutionService::new(PaperLedger::new(), StubProducer::new());
    let submission = service
        .submit_persisted(&store, intent.clone(), &book, Some(opportunity_id))
        .await
        .unwrap();
    assert!(!submission.replayed);

    let duplicate =
        service.submit_persisted(&store, intent, &book, Some(opportunity_id)).await.unwrap();
    assert!(duplicate.replayed);
    let paper_orders: i64 =
        sqlx::query_scalar("SELECT count(*) FROM orders WHERE order_id=$1 AND paper=true")
            .bind(order_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let live_rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM orders WHERE order_id=$1 AND paper=false) + \
                (SELECT count(*) FROM fills WHERE order_id=$1 AND paper=false)",
    )
    .bind(order_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paper_orders, 1);
    assert_eq!(live_rows, 0);

    store
        .close_opportunity(&AttributionClose {
            opportunity_id,
            predicted_net_edge: Decimal::new(10, 2),
            realized_pnl: Decimal::new(8, 2),
            outcome: "closed_by_test".to_owned(),
            components: AttributionComponents {
                predicted_fees: Decimal::new(1, 2),
                realized_fees: Decimal::new(2, 2),
                predicted_slippage: Decimal::ZERO,
                realized_slippage: Decimal::new(1, 2),
                predicted_funding: None,
                realized_funding: None,
            },
        })
        .await
        .unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM opportunities WHERE id=$1")
        .bind(opportunity_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM opportunity_events WHERE opportunity_id=$1")
            .bind(opportunity_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    let detail: serde_json::Value =
        sqlx::query_scalar("SELECT detail FROM attribution WHERE opportunity_id=$1")
            .bind(opportunity_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "closed");
    assert_eq!(event_count, 2);
    assert_eq!(detail["fees"]["divergence"], "0.01");

    cleanup(&pool, opportunity_id, order_id, market.as_str(), &venue_slug).await;
}

fn make_book(market: MarketKey) -> OrderBook {
    OrderBook::new(
        market,
        vec![BookLevel { price: Decimal::new(99, 2), size: Decimal::new(100, 0) }],
        vec![BookLevel { price: Decimal::new(101, 2), size: Decimal::new(100, 0) }],
        1,
        UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
        None,
    )
    .unwrap()
}

fn make_intent(market: MarketKey) -> OrderIntent {
    OrderIntent {
        id: Ulid::new(),
        market: market.clone(),
        side: Side::Buy,
        order_type: OrderType::Market,
        limit_price: None,
        size: Decimal::new(10, 0),
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper: true,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: Quote {
            market,
            bid: Some(Decimal::new(99, 2)),
            ask: Some(Decimal::new(101, 2)),
            mid: Some(Decimal::ONE),
            last: None,
            bid_size: Some(Decimal::new(100, 0)),
            ask_size: Some(Decimal::new(100, 0)),
            ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
            source: QuoteSource::Snapshot,
            seq: None,
        },
        caps_version: Ulid::new(),
        created_ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
    }
}

async fn cleanup(
    pool: &sqlx::PgPool,
    opportunity_id: Ulid,
    order_id: Ulid,
    market: &str,
    venue: &str,
) {
    for statement in [
        "DELETE FROM attribution WHERE opportunity_id=$1",
        "DELETE FROM opportunity_events WHERE opportunity_id=$1",
        "DELETE FROM opportunities WHERE id=$1",
    ] {
        sqlx::query(statement).bind(opportunity_id.to_string()).execute(pool).await.unwrap();
    }
    sqlx::query("DELETE FROM fills WHERE order_id=$1")
        .bind(order_id.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM positions WHERE market_key=$1")
        .bind(market)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM orders WHERE order_id=$1")
        .bind(order_id.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM order_intents WHERE id=$1")
        .bind(order_id.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM markets WHERE key=$1").bind(market).execute(pool).await.unwrap();
    sqlx::query("DELETE FROM venues WHERE slug=$1").bind(venue).execute(pool).await.unwrap();
}
