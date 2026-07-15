#![allow(clippy::unwrap_used)]

use aether_bus::producer::StubProducer;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::order::Fill;
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_fillmodel::config::FillConfig;
use aether_paper_ledger::ledger::PaperLedger;
use aether_paper_ledger::pnl::PnLCalculator;
use aether_paper_ledger::positions::PositionTracker;
use aether_paper_ledger::service::PaperExecutionService;
use rust_decimal::Decimal;

fn make_book() -> OrderBook {
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let ts = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    let bids = vec![BookLevel { price: Decimal::new(99, 2), size: Decimal::new(100, 0) }];
    let asks = vec![
        BookLevel { price: Decimal::new(101, 2), size: Decimal::new(50, 0) },
        BookLevel { price: Decimal::new(102, 2), size: Decimal::new(50, 0) },
    ];
    OrderBook::new(mkt, bids, asks, 2, ts, None).unwrap()
}

fn make_intent(size: i64, paper: bool) -> OrderIntent {
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let _ts = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    OrderIntent {
        id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        order_type: OrderType::Market,
        limit_price: None,
        size: Decimal::new(size, 0),
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: Quote {
            market: mkt,
            bid: None,
            ask: None,
            mid: None,
            last: None,
            bid_size: None,
            ask_size: None,
            ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
            source: QuoteSource::Stream,
            seq: None,
        },
        caps_version: Ulid::new(),
        created_ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
    }
}

#[test]
fn ledger_accepts_paper_intent_and_produces_fills() {
    let mut ledger = PaperLedger::new();
    let book = make_book();
    let intent = make_intent(60, true);
    let fills = ledger.submit(intent, &book).unwrap();

    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].size, Decimal::new(50, 0));
    assert_eq!(fills[1].size, Decimal::new(10, 0));
    assert_eq!(ledger.orders().len(), 1);
    assert_eq!(ledger.fills().len(), 2);
}

#[test]
fn ledger_rejects_non_paper_intent() {
    let mut ledger = PaperLedger::new();
    let book = make_book();
    let intent = make_intent(10, false); // not paper
    let result = ledger.submit(intent, &book);
    assert!(result.is_err());
}

#[test]
fn ledger_replays_duplicate_intent_id_without_double_fill() {
    let mut ledger = PaperLedger::new();
    let book = make_book();
    let id = Ulid::new();
    let intent1 = {
        let mut i = make_intent(10, true);
        i.id = id;
        i
    };
    let intent2 = intent1.clone();
    let first = ledger.execute(intent1, &book).unwrap();
    let replay = ledger.execute(intent2, &book).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(first.fills, replay.fills);
    assert_eq!(ledger.fills().len(), first.fills.len());
}

#[test]
fn ledger_cancel_rejects_filled_order() {
    let mut ledger = PaperLedger::new();
    let book = make_book();
    let intent = make_intent(100, true);
    let id = intent.id;
    let _fills = ledger.submit(intent, &book).unwrap();
    let result = ledger.cancel(&id);
    assert!(result.is_err()); // Filled, cannot cancel
}

#[test]
fn position_tracks_long_buy() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let fill = Fill {
        order_id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        price: Decimal::new(100, 2), // 1.00
        size: Decimal::new(10, 0),   // 10 contracts
        fee: aether_core::ids::Money::new(Decimal::ZERO, "USDC"),
        venue_ref: JsonObject::default(),
        ts: UtcTime::now(),
        paper: true,
    };
    tracker.apply_fill(&fill).unwrap();

    let pos = tracker.get(&mkt).unwrap();
    assert_eq!(pos.net_size, Decimal::new(10, 0));
    assert_eq!(pos.avg_entry_price, Decimal::new(100, 2));
    assert_eq!(pos.fill_count, 1);
}

#[test]
fn position_realizes_pnl_on_close() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();

    // Buy 10 @ 1.00
    let buy = Fill {
        order_id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        price: Decimal::new(100, 2),
        size: Decimal::new(10, 0),
        fee: aether_core::ids::Money::new(Decimal::ZERO, "USDC"),
        venue_ref: JsonObject::default(),
        ts: UtcTime::now(),
        paper: true,
    };
    tracker.apply_fill(&buy).unwrap();

    // Sell 10 @ 1.05 -> +0.05 x 10 = 0.50 profit
    let sell = Fill {
        order_id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Sell,
        price: Decimal::new(105, 2),
        size: Decimal::new(10, 0),
        fee: aether_core::ids::Money::new(Decimal::ZERO, "USDC"),
        venue_ref: JsonObject::default(),
        ts: UtcTime::now(),
        paper: true,
    };
    tracker.apply_fill(&sell).unwrap();

    let pos = tracker.get(&mkt).unwrap();
    assert_eq!(pos.net_size, Decimal::ZERO); // closed
    assert!(pos.realized_pnl > Decimal::ZERO); // profit
}

#[test]
fn pnl_calculator_records_fills() {
    let mut calc = PnLCalculator::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let fill = Fill {
        order_id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        price: Decimal::new(100, 2),
        size: Decimal::new(10, 0),
        fee: aether_core::ids::Money::new(Decimal::new(10, 2), "USDC"), // 0.10 fee
        venue_ref: JsonObject::default(),
        ts: UtcTime::now(),
        paper: true,
    };
    calc.record_fills(&[fill]).unwrap();
    assert_eq!(calc.total_fills(), 1);
    assert_eq!(calc.total_fees(), Decimal::new(10, 2)); // 0.10
}

#[test]
fn pnl_summary_with_mark_price() {
    let mut tracker = PositionTracker::new();
    let mut calc = PnLCalculator::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();

    // Buy 10 @ 1.00
    let buy = Fill {
        order_id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        price: Decimal::new(100, 2),
        size: Decimal::new(10, 0),
        fee: aether_core::ids::Money::new(Decimal::ZERO, "USDC"),
        venue_ref: JsonObject::default(),
        ts: UtcTime::now(),
        paper: true,
    };
    tracker.apply_fill(&buy).unwrap();
    calc.record_fills(&[buy]).unwrap();

    // Update mark to 1.10 -> +0.10 x 10 = 1.00 unrealized
    calc.update_mark(&mkt, Decimal::new(110, 2));

    let summary = calc.compute_summary(&tracker).unwrap();
    assert_eq!(summary.total_realized, Decimal::ZERO);
    assert_eq!(summary.total_unrealized, Decimal::new(100, 2)); // 1.00
    assert_eq!(summary.per_market.len(), 1);
}

#[test]
fn ledger_with_passive_config() {
    let config = FillConfig::passive();
    let mut ledger = PaperLedger::with_config(config);
    let book = make_book();
    let intent = make_intent(100, true);
    let fills = ledger.submit(intent, &book).unwrap();
    // Passive at touch: only one fill at best ask level
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].size, Decimal::new(50, 0));
}

#[test]
fn quote_midpoint_updates_unrealized_pnl() {
    let mut ledger = PaperLedger::new();
    let book = make_book();
    let intent = make_intent(10, true);
    ledger.submit(intent, &book).unwrap();
    let before = ledger.pnl().compute_summary(ledger.positions()).unwrap().total_unrealized;
    let market = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let quote = Quote {
        market,
        bid: Some(Decimal::new(109, 2)),
        ask: Some(Decimal::new(111, 2)),
        mid: Some(Decimal::new(110, 2)),
        last: None,
        bid_size: None,
        ask_size: None,
        ts: UtcTime::from_unix_millis(1_750_000_001_000).unwrap(),
        source: QuoteSource::Stream,
        seq: Some(2),
    };
    ledger.update_quote(&quote).unwrap();
    let after = ledger.pnl().compute_summary(ledger.positions()).unwrap().total_unrealized;

    assert_eq!(before, Decimal::ZERO);
    assert_eq!(after, Decimal::new(90, 2));
}

#[test]
fn failed_fill_does_not_leave_ghost_order() {
    let mut ledger = PaperLedger::new();
    let empty_book = OrderBook::new(
        MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap(),
        vec![],
        vec![],
        0,
        UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
        None,
    )
    .unwrap();
    let intent = make_intent(10, true);

    assert!(ledger.submit(intent, &empty_book).is_err());
    assert!(ledger.orders().is_empty());
}

#[test]
fn partial_long_close_realizes_pnl_without_changing_average() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Buy, 100, 10, 0)).unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Sell, 110, 4, 0)).unwrap();

    let pos = tracker.get(&mkt).unwrap();
    assert_eq!(pos.net_size, Decimal::new(6, 0));
    assert_eq!(pos.avg_entry_price, Decimal::new(100, 2));
    assert_eq!(pos.realized_pnl, Decimal::new(40, 2));
}

#[test]
fn short_accumulation_uses_absolute_vwap() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Sell, 100, 10, 0)).unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Sell, 80, 10, 0)).unwrap();

    let pos = tracker.get(&mkt).unwrap();
    assert_eq!(pos.net_size, Decimal::new(-20, 0));
    assert_eq!(pos.avg_entry_price, Decimal::new(90, 2));
}

#[test]
fn fees_reduce_realized_pnl() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Buy, 100, 10, 10)).unwrap();
    tracker.apply_fill(&make_fill(&mkt, Side::Sell, 110, 10, 10)).unwrap();

    let pos = tracker.get(&mkt).unwrap();
    assert_eq!(pos.realized_pnl, Decimal::new(80, 2));
    assert_eq!(pos.fees_paid, Decimal::new(20, 2));
}

#[test]
fn position_overflow_does_not_partially_mutate_state() {
    let mut tracker = PositionTracker::new();
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let first = Fill { price: Decimal::MAX, ..make_fill(&mkt, Side::Buy, 100, 1, 0) };
    let second = first.clone();
    tracker.apply_fill(&first).unwrap();

    assert!(tracker.apply_fill(&second).is_err());
    let position = tracker.get(&mkt).unwrap();
    assert_eq!(position.net_size, Decimal::ONE);
    assert_eq!(position.fill_count, 1);
}

fn make_fill(market: &MarketKey, side: Side, price: i64, size: i64, fee: i64) -> Fill {
    Fill {
        order_id: Ulid::new(),
        market: market.clone(),
        side,
        price: Decimal::new(price, 2),
        size: Decimal::new(size, 0),
        fee: aether_core::ids::Money::new(Decimal::new(fee, 2), "USDC"),
        venue_ref: JsonObject::default(),
        ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
        paper: true,
    }
}

#[tokio::test]
async fn service_emits_each_fill_once_on_registered_topic() {
    let producer = StubProducer::new();
    let sent = producer.sent.clone();
    let mut service = PaperExecutionService::new(PaperLedger::new(), producer);
    let book = make_book();
    let intent = make_intent(60, true);

    let first = service.submit(intent.clone(), &book).await.unwrap();
    let replay = service.submit(intent, &book).await.unwrap();

    assert!(!first.replayed);
    assert!(replay.replayed);
    let messages = sent.lock().unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|(topic, _)| topic == "orders.fills"));
}

#[test]
fn ledger_side_satisfies_shared_fill_parity_contract() {
    let book = make_book();
    let intent = make_intent(60, true);
    let expected = aether_fillmodel::walk(&book, &intent, &FillConfig::default()).unwrap();
    let actual = PaperLedger::new().submit(intent, &book).unwrap();
    assert_eq!(actual, expected);
}
