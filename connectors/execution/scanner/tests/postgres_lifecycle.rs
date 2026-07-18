#![allow(clippy::unwrap_used)]

use aether_bus::envelope::Envelope;
use aether_bus::producer::StubProducer;
use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_scanner::{
    EvidenceSignal, EvidenceSnapshot, LifecycleStore, PersistDisposition, ScanConfig, Scanner,
    ScannerRuntime,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct ReplayFixture {
    now: UtcTime,
    trace_id: String,
    evidence_object_id: Ulid,
    event_id: String,
    markets: Vec<ReplayMarket>,
}

#[derive(Deserialize)]
struct ReplayMarket {
    key: MarketKey,
    venue: VenueId,
    #[serde(with = "aether_core::decimal::decimal_string")]
    bid: Decimal,
    #[serde(with = "aether_core::decimal::decimal_string")]
    ask: Decimal,
    #[serde(with = "aether_core::decimal::decimal_string")]
    bid_size: Decimal,
    #[serde(with = "aether_core::decimal::decimal_string")]
    ask_size: Decimal,
}

fn opportunity(id: Ulid, detected_ms: i64, first: &str, second: &str) -> Opportunity {
    let buy = MarketKey::new(&VenueId::new("kalshi").unwrap(), first).unwrap();
    let sell = MarketKey::new(&VenueId::new("polymarket").unwrap(), second).unwrap();
    Opportunity {
        id,
        kind: OpportunityKind::Arbitrage,
        legs: vec![
            OpportunityLeg {
                market: buy,
                side: Side::Buy,
                target_price: Some(Decimal::new(62, 2)),
                size_hint: Some(Decimal::ONE),
            },
            OpportunityLeg {
                market: sell,
                side: Side::Sell,
                target_price: Some(Decimal::new(67, 2)),
                size_hint: Some(Decimal::ONE),
            },
        ],
        gross_edge: Decimal::new(5, 2),
        edge: EdgeDecomposition::compute(Decimal::new(5, 2), EdgeCosts::zero()),
        confidence: Confidence::new(Decimal::new(9, 1)).unwrap(),
        detected_ts: UtcTime::from_unix_millis(detected_ms).unwrap(),
        expires_ts: UtcTime::from_unix_millis(detected_ms + 30_000).ok(),
        explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "fixture".into() },
        trace_id: Ulid::new(),
    }
}

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn lifecycle_dedupe_outbox_expiry_and_attribution_are_durable() {
    assert_eq!(std::env::var("AETHER_INTEGRATION_TEST").as_deref(), Ok("1"));
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
    let store = LifecycleStore::new(pool.clone());
    let suffix = Ulid::new().to_string();
    let first_id = Ulid::new();
    let first = opportunity(first_id, 1_750_000_000_000, &suffix, "shared");
    assert_eq!(store.persist_scored(&first).await.unwrap(), PersistDisposition::Created(first_id));

    let duplicate = opportunity(Ulid::new(), 1_750_000_001_000, &suffix, "shared");
    assert_eq!(
        store.persist_scored(&duplicate).await.unwrap(),
        PersistDisposition::Updated(first_id)
    );
    let row: (String, i64, i64) = sqlx::query_as(
        "SELECT state, \
         (SELECT count(*) FROM opportunity_events WHERE opportunity_id=$1), \
         (SELECT count(*) FROM opportunity_detection_outbox WHERE opportunity_id=$1) \
         FROM opportunities WHERE id=$1",
    )
    .bind(first_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("scored".to_owned(), 1, 1));

    assert!(sqlx::query("UPDATE opportunities SET state='accepted' WHERE id=$1")
        .bind(first_id.to_string())
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query(
        "INSERT INTO opportunity_events (id,opportunity_id,from_state,to_state,actor) \
         VALUES ($1,$2,'scored','accepted','test')",
    )
    .bind(Ulid::new().to_string())
    .bind(first_id.to_string())
    .execute(&pool)
    .await
    .is_err());

    let producer = StubProducer::new();
    assert_eq!(store.flush_detection_outbox(&producer, 10).await.unwrap(), 1);
    assert_eq!(store.flush_detection_outbox(&producer, 10).await.unwrap(), 0);
    assert_eq!(producer.sent.lock().unwrap().len(), 1);
    sqlx::query(
        "INSERT INTO opportunity_events (id,opportunity_id,from_state,to_state,actor) \
         VALUES ($1,$2,'scored','surfaced','gateway')",
    )
    .bind(Ulid::new().to_string())
    .bind(first_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let expired_id = Ulid::new();
    let expired = opportunity(expired_id, 1_750_000_100_000, &format!("x{suffix}"), "expiry");
    store.persist_scored(&expired).await.unwrap();
    let swept =
        store.expire_due(UtcTime::from_unix_millis(1_750_000_130_001).unwrap()).await.unwrap();
    assert_eq!(swept, vec![expired_id]);
    let closure: (String, String, i64) = sqlx::query_as(
        "SELECT o.state,a.outcome, \
         (SELECT count(*) FROM opportunity_events WHERE opportunity_id=o.id) \
         FROM opportunities o JOIN attribution a ON a.opportunity_id=o.id WHERE o.id=$1",
    )
    .bind(expired_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(closure, ("closed".to_owned(), "expired_pre_surface".to_owned(), 3));

    for id in [first_id, expired_id] {
        sqlx::query("DELETE FROM opportunity_detection_outbox WHERE opportunity_id=$1")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM attribution WHERE opportunity_id=$1")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM opportunity_events WHERE opportunity_id=$1")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM opportunities WHERE id=$1")
            .bind(id.to_string())
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn recorded_three_venue_runtime_is_restart_safe_and_closes_with_attribution() {
    assert_eq!(std::env::var("AETHER_INTEGRATION_TEST").as_deref(), Ok("1"));
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
    let store = LifecycleStore::new(pool.clone());
    let fixture: ReplayFixture = serde_json::from_str(include_str!(
        "../../../../testdata/scanner/phase1-normalized-replay.json"
    ))
    .unwrap();
    let fallback = BrainRef {
        object_id: fixture.evidence_object_id,
        provenance_hash: "phase1-recording".into(),
    };
    let evidence = HashMap::from([(
        format!("id:{}", fixture.event_id),
        EvidenceSnapshot {
            signal: EvidenceSignal { relevance: Decimal::ONE, trust: Decimal::ONE },
            explain_ref: fallback.clone(),
        },
    )]);
    let markets: Vec<Market> = fixture
        .markets
        .iter()
        .map(|row| Market {
            key: row.key.clone(),
            venue: row.venue.clone(),
            kind: InstrumentKind::BinaryContract,
            title: fixture.event_id.clone(),
            description_ref: String::new(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::new(serde_json::json!({"canonical_event_id": fixture.event_id}))
                .unwrap(),
        })
        .collect();
    let feed = |runtime: &mut ScannerRuntime| {
        for row in &fixture.markets {
            runtime
                .apply_quote_envelope(Envelope {
                    schema: "aether.quote.v1".into(),
                    trace_id: fixture.trace_id.clone(),
                    ts: fixture.now.to_string(),
                    payload: Quote {
                        market: row.key.clone(),
                        bid: Some(row.bid),
                        ask: Some(row.ask),
                        mid: Some((row.bid + row.ask) / Decimal::new(2, 0)),
                        last: None,
                        bid_size: Some(row.bid_size),
                        ask_size: Some(row.ask_size),
                        ts: fixture.now,
                        source: QuoteSource::Stream,
                        seq: Some(1),
                    },
                })
                .unwrap();
            runtime
                .apply_book_envelope(Envelope {
                    schema: "aether.order_book.v1".into(),
                    trace_id: fixture.trace_id.clone(),
                    ts: fixture.now.to_string(),
                    payload: OrderBook::new(
                        row.key.clone(),
                        vec![BookLevel { price: row.bid, size: row.bid_size }],
                        vec![BookLevel { price: row.ask, size: row.ask_size }],
                        1,
                        fixture.now,
                        Some(1),
                    )
                    .unwrap(),
                })
                .unwrap();
        }
    };

    let producer = StubProducer::new();
    let mut runtime =
        ScannerRuntime::new(Scanner::new(ScanConfig::default()).unwrap(), fallback.clone());
    runtime.replace_markets(markets.clone());
    runtime.replace_evidence(evidence.clone());
    feed(&mut runtime);
    let first = runtime.run_due(fixture.now, &store, &producer).await.unwrap();
    assert_eq!(first.durable.created, 3);
    assert_eq!(first.durable.published, 3);

    let mut restarted = ScannerRuntime::new(Scanner::new(ScanConfig::default()).unwrap(), fallback);
    restarted.replace_markets(markets);
    restarted.replace_evidence(evidence);
    feed(&mut restarted);
    let replay = restarted.run_due(fixture.now, &store, &producer).await.unwrap();
    assert_eq!(replay.durable.updated, 3);
    assert_eq!(replay.durable.published, 0);
    assert_eq!(producer.sent.lock().unwrap().len(), 3);

    let after_ttl = UtcTime::from_unix_millis(fixture.now.unix_millis() + 30_001).unwrap();
    let closed = restarted.run_due(after_ttl, &store, &producer).await.unwrap();
    assert_eq!(closed.expired.len(), 3);
    assert_eq!(closed.open_chains, 0);
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM opportunities WHERE trace_id=$1 ORDER BY id")
            .bind(&fixture.trace_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ids.len(), 3);
    for id in ids {
        let evidence: (String, i64, i64) = sqlx::query_as(
            "SELECT state, \
             (SELECT count(*) FROM opportunity_events WHERE opportunity_id=$1), \
             (SELECT count(*) FROM attribution WHERE opportunity_id=$1) \
             FROM opportunities WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(evidence, ("closed".into(), 3, 1));
        sqlx::query("DELETE FROM opportunity_detection_outbox WHERE opportunity_id=$1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM attribution WHERE opportunity_id=$1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM opportunity_events WHERE opportunity_id=$1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM opportunities WHERE id=$1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
