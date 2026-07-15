#![allow(clippy::unwrap_used)]

use aether_authz::{Actor, ActorKind, AuditRecord, EvaluationContext, Grant, Tier};
use aether_core::ids::{MarketKey, Money, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::order::{
    CapsSnapshot, OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce,
};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_order_router::adapter::{
    AdapterError, VenueAdapterClient, VenueBalances, VenueOrderResult,
};
use aether_order_router::reconciler::ReconciledStatus;
use aether_order_router::router::{
    ExecutionAuditError, ExecutionAuditEvent, ExecutionAuditSink, MemoryExecutionAuditSink,
    OrderRouter, RouterAuthContext, RouterConfig, RouterError, RouterResult,
};
use aether_risk_engine::engine::{Balances, RiskConfig, RiskContext, VenueHealthStatus};
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Creates a RouterConfig with staleness check disabled for test fixtures.
fn test_router_config() -> RouterConfig {
    RouterConfig {
        risk_config: RiskConfig {
            max_quote_staleness_ms: 99_999_999_999, // ~3,170 years -- disabled for tests
            ..RiskConfig::default()
        },
        ..RouterConfig::default()
    }
}

fn make_book() -> OrderBook {
    let mkt = MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap();
    let ts = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    let bids = vec![BookLevel { price: Decimal::new(99, 2), size: Decimal::new(100, 0) }];
    let asks = vec![BookLevel { price: Decimal::new(101, 2), size: Decimal::new(50, 0) }];
    OrderBook::new(mkt, bids, asks, 2, ts, None).unwrap()
}

fn make_intent(paper: bool) -> OrderIntent {
    let venue = VenueId::new("test").unwrap();
    let mkt = MarketKey::new(&venue, "TEST").unwrap();
    let ts = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    OrderIntent {
        id: Ulid::new(),
        market: mkt.clone(),
        side: Side::Buy,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::new(101, 2)),
        size: Decimal::new(10, 0),
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: Quote {
            market: mkt,
            bid: Some(Decimal::new(99, 2)),
            ask: Some(Decimal::new(101, 2)),
            mid: None,
            last: None,
            bid_size: None,
            ask_size: None,
            ts,
            source: QuoteSource::Stream,
            seq: None,
        },
        caps_version: caps_version(),
        created_ts: ts,
    }
}

fn caps_version() -> Ulid {
    Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
}

fn make_router() -> (OrderRouter, Arc<MemoryExecutionAuditSink>) {
    let audit = Arc::new(MemoryExecutionAuditSink::default());
    (OrderRouter::new(test_router_config(), audit.clone()), audit)
}

fn submit_authorized(
    router: &OrderRouter,
    intent: OrderIntent,
    book: &OrderBook,
    context: &RiskContext,
) -> Result<RouterResult, RouterError> {
    let actor = Actor { id: intent.origin.actor_id.to_string(), kind: ActorKind::Agent };
    let grant = Grant {
        id: "test-grant".into(),
        actor_id: actor.id.clone(),
        actor_kind: actor.kind,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::new(),
        scope_restricted: false,
        expires_at: None,
        revoked_at: None,
    };
    let mut evaluation = EvaluationContext::new(1_750_000_000, Some(&grant));
    evaluation.confirmed = true;
    evaluation.step_up_satisfied = true;
    evaluation.live_history_days = 30;
    router.submit(intent, book, context, &RouterAuthContext { actor: &actor, evaluation })
}

fn make_risk_context() -> RiskContext {
    let venue = VenueId::new("test").unwrap();
    let market_key = MarketKey::new(&venue, "TEST").unwrap();
    let evaluated_at = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    let mut balances = HashMap::new();
    balances.insert(
        venue.clone(),
        Balances { free: Decimal::new(100_000, 2), locked: Decimal::ZERO, currency: "USD".into() },
    );
    let mut health = HashMap::new();
    health.insert(venue.clone(), VenueHealthStatus { status: "ok".into(), breaker_open: false });
    let active_caps = CapsSnapshot {
        version: Ulid::new(),
        per_order_max: Money::new(Decimal::new(500_000, 2), "USD"),
        daily_max: Money::new(Decimal::new(5_000_000, 2), "USD"),
        per_venue: JsonObject::default(),
        per_kind: JsonObject::default(),
    };
    let mut caps_by_version = HashMap::new();
    caps_by_version.insert(caps_version(), active_caps.clone());
    let mut markets = HashMap::new();
    markets.insert(
        market_key.clone(),
        Market {
            key: market_key,
            venue,
            kind: InstrumentKind::BinaryContract,
            title: "test".into(),
            description_ref: "test".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: Vec::new(),
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        },
    );
    RiskContext {
        evaluated_at,
        markets,
        balances,
        positions: HashMap::new(),
        venue_health: health,
        active_caps,
        caps_by_version,
        daily_notional: Decimal::ZERO,
        jurisdiction_eligible: HashMap::new(),
        live_enabled: false,
    }
}

#[test]
fn router_submits_paper_order_through_risk_and_ledger() {
    let (router, _) = make_router();
    let book = make_book();
    let intent = make_intent(true);
    let ctx = make_risk_context();
    let result = submit_authorized(&router, intent, &book, &ctx).unwrap();
    assert!(!result.replayed);
    assert_eq!(result.fills.len(), 1);
    assert!(result.verdict.is_allowed());
}

#[test]
fn router_rejects_risky_order() {
    let (router, _) = make_router();
    let book = make_book();
    let mut intent = make_intent(true);
    intent.limit_price = Some(Decimal::new(120, 2)); // 1.20 -- adverse buy drift
    let ctx = make_risk_context();
    let result = submit_authorized(&router, intent, &book, &ctx);
    assert!(matches!(result, Err(RouterError::RiskDenied(_))));
}

#[test]
fn router_replays_duplicate_intent_id() {
    let (router, _) = make_router();
    let book = make_book();
    let intent = make_intent(true);
    let ctx = make_risk_context();

    let first = submit_authorized(&router, intent.clone(), &book, &ctx).unwrap();
    assert!(!first.replayed);

    let second = submit_authorized(&router, intent, &book, &ctx).unwrap();
    assert!(second.replayed, "duplicate intent should be replayed");
    assert_eq!(first.fills.len(), second.fills.len());
}

#[test]
fn router_blocks_live_order_when_disabled() {
    let (router, _) = make_router();
    let book = make_book();
    let intent = make_intent(false); // live
    let ctx = make_risk_context(); // live_enabled = false
    let result = submit_authorized(&router, intent, &book, &ctx);
    assert!(matches!(result, Err(RouterError::RiskDenied(_))));
}

#[test]
fn router_allows_paper_when_live_disabled() {
    let (router, _) = make_router();
    let book = make_book();
    let intent = make_intent(true); // paper
    let ctx = make_risk_context(); // live_enabled = false
    assert!(submit_authorized(&router, intent, &book, &ctx).is_ok());
}

#[test]
fn router_idempotent_submit_one_fill_per_intent() {
    let (router, audit) = make_router();
    let book = make_book();
    let intent = make_intent(true);
    let ctx = make_risk_context();

    submit_authorized(&router, intent.clone(), &book, &ctx).unwrap();
    submit_authorized(&router, intent.clone(), &book, &ctx).unwrap();
    submit_authorized(&router, intent, &book, &ctx).unwrap();

    // Only one order recorded, one set of fills
    let (orders, fills) = router.paper_counts().unwrap();
    assert_eq!(fills, 1, "only one fill set despite 3 submits");
    assert_eq!(orders, 1, "only one order despite 3 submits");
    assert_eq!(audit.authz_records().len(), 1, "replays do not repeat authorization");
}

#[test]
fn router_rechecks_authorization_and_caches_denial() {
    let (router, audit) = make_router();
    let intent = make_intent(true);
    let actor = Actor { id: intent.origin.actor_id.to_string(), kind: ActorKind::Agent };
    let grant = Grant {
        id: "read-only".into(),
        actor_id: actor.id.clone(),
        actor_kind: actor.kind,
        tier: Tier::ReadOnly,
        scopes: HashSet::new(),
        scope_restricted: false,
        expires_at: None,
        revoked_at: None,
    };
    let evaluation = EvaluationContext::new(1_750_000_000, Some(&grant));
    let auth = RouterAuthContext { actor: &actor, evaluation };
    let first = router.submit(intent.clone(), &make_book(), &make_risk_context(), &auth);
    assert_eq!(first, Err(RouterError::Unauthorized("tier.insufficient")));
    let second = router.submit(intent, &make_book(), &make_risk_context(), &auth);
    assert_eq!(first, second, "a duplicate cannot change a terminal denial");
    assert_eq!(audit.authz_records().len(), 1);
    assert_eq!(router.paper_counts().unwrap(), (0, 0));
}

struct FailingAudit;
impl ExecutionAuditSink for FailingAudit {
    fn emit_authz(&self, _record: &AuditRecord) -> Result<(), ExecutionAuditError> {
        Err(ExecutionAuditError)
    }
    fn emit_execution(&self, _event: &ExecutionAuditEvent) -> Result<(), ExecutionAuditError> {
        Err(ExecutionAuditError)
    }
}

struct MockLiveAdapter {
    submit_result: Result<(), AdapterError>,
    query_result: Mutex<Result<(String, bool), AdapterError>>,
    submitted_ids: Mutex<Vec<Ulid>>,
    queried_refs: Mutex<Vec<String>>,
}

impl MockLiveAdapter {
    fn accepted() -> Self {
        Self {
            submit_result: Ok(()),
            query_result: Mutex::new(Ok(("accepted".into(), true))),
            submitted_ids: Mutex::new(Vec::new()),
            queried_refs: Mutex::new(Vec::new()),
        }
    }

    fn timeout() -> Self {
        Self {
            submit_result: Err(AdapterError::Timeout),
            query_result: Mutex::new(Err(AdapterError::Timeout)),
            submitted_ids: Mutex::new(Vec::new()),
            queried_refs: Mutex::new(Vec::new()),
        }
    }

    fn timeout_then_query(status: &str, accepted: bool) -> Self {
        Self {
            submit_result: Err(AdapterError::Timeout),
            query_result: Mutex::new(Ok((status.into(), accepted))),
            submitted_ids: Mutex::new(Vec::new()),
            queried_refs: Mutex::new(Vec::new()),
        }
    }

    fn timeout_then_not_found() -> Self {
        Self {
            submit_result: Err(AdapterError::Timeout),
            query_result: Mutex::new(Err(AdapterError::NotFound("missing".into()))),
            submitted_ids: Mutex::new(Vec::new()),
            queried_refs: Mutex::new(Vec::new()),
        }
    }

    fn submitted_count(&self) -> usize {
        self.submitted_ids.lock().unwrap().len()
    }

    fn queried_count(&self) -> usize {
        self.queried_refs.lock().unwrap().len()
    }
}

impl VenueAdapterClient for MockLiveAdapter {
    fn submit_order(&self, intent: &OrderIntent) -> Result<VenueOrderResult, AdapterError> {
        self.submitted_ids.lock().unwrap().push(intent.id);
        self.submit_result.clone().map(|()| VenueOrderResult {
            venue_ref: format!("venue-order-{}", intent.id),
            client_order_id: intent.id.to_string(),
            status: "accepted".into(),
            accepted: true,
        })
    }

    fn cancel_order(&self, _venue_ref: &str) -> Result<(), AdapterError> {
        Ok(())
    }

    fn query_order(&self, venue_ref: &str) -> Result<VenueOrderResult, AdapterError> {
        self.queried_refs.lock().unwrap().push(venue_ref.into());
        self.query_result.lock().unwrap().clone().map(|(status, accepted)| VenueOrderResult {
            venue_ref: format!("venue-order-{venue_ref}"),
            client_order_id: venue_ref.into(),
            status,
            accepted,
        })
    }

    fn get_balances(&self) -> Result<VenueBalances, AdapterError> {
        Ok(VenueBalances {
            free: Decimal::new(100_000, 2),
            locked: Decimal::ZERO,
            currency: "USD".into(),
        })
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

#[test]
fn read_only_venue_rejects_live_order() {
    let (mut router, _audit) = make_router();
    // Re-register test venue with ReadOnlyAdapter
    let vid = VenueId::new("test").unwrap();
    router.register_adapter(
        vid.clone(),
        Arc::new(aether_order_router::adapter::ReadOnlyAdapter { venue_id: vid }),
    );
    let intent = make_intent(false); // live
    let mut ctx = make_risk_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);
    let result = submit_authorized(&router, intent, &make_book(), &ctx);
    assert!(matches!(result, Err(RouterError::VenueNotImplemented)));
}

#[test]
fn live_order_dispatches_to_registered_adapter_once() {
    let (mut router, audit) = make_router();
    let adapter = Arc::new(MockLiveAdapter::accepted());
    router.register_adapter(VenueId::new("test").unwrap(), adapter.clone());
    let intent = make_intent(false);
    let mut ctx = make_risk_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);

    let result = submit_authorized(&router, intent.clone(), &make_book(), &ctx).unwrap();
    assert!(result.verdict.is_allowed());
    assert!(result.fills.is_empty(), "live fills arrive from venue observations");
    assert_eq!(adapter.submitted_count(), 1);

    let replay = submit_authorized(&router, intent, &make_book(), &ctx).unwrap();
    assert!(replay.replayed);
    assert_eq!(adapter.submitted_count(), 1, "idempotent replay must not submit twice");
    assert!(audit
        .execution_records()
        .iter()
        .any(|event| event.phase == "live_submit_accepted" && event.allowed));
}

#[test]
fn live_timeout_enters_unknown_reconciliation_without_retrying() {
    let (mut router, audit) = make_router();
    let adapter = Arc::new(MockLiveAdapter::timeout());
    router.register_adapter(VenueId::new("test").unwrap(), adapter.clone());
    let intent = make_intent(false);
    let mut ctx = make_risk_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);

    let first = submit_authorized(&router, intent.clone(), &make_book(), &ctx);
    assert_eq!(first, Err(RouterError::SubmitUnknown));
    assert_eq!(router.reconciliation_status(&intent.id).unwrap(), Some(ReconciledStatus::Unknown));
    assert_eq!(router.pending_unknown_count().unwrap(), 1);

    let second = submit_authorized(&router, intent, &make_book(), &ctx);
    assert_eq!(second, Err(RouterError::SubmitUnknown));
    assert_eq!(adapter.submitted_count(), 1, "terminal unknown result is replayed");
    assert_eq!(adapter.queried_count(), 0, "submit replay must not query or resubmit");
    assert!(audit
        .execution_records()
        .iter()
        .any(|event| event.phase == "live_submit_unknown" && !event.allowed));
}

#[test]
fn reconcile_unknown_queries_adapter_by_client_order_id() {
    let (mut router, audit) = make_router();
    let adapter = Arc::new(MockLiveAdapter::timeout_then_query("filled", true));
    router.register_adapter(VenueId::new("test").unwrap(), adapter.clone());
    let intent = make_intent(false);
    let mut ctx = make_risk_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);

    assert_eq!(
        submit_authorized(&router, intent.clone(), &make_book(), &ctx),
        Err(RouterError::SubmitUnknown)
    );

    let updates = router.reconcile_unknowns(ctx.evaluated_at).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].intent_id, intent.id);
    assert_eq!(updates[0].status, ReconciledStatus::Filled);
    assert_eq!(router.reconciliation_status(&intent.id).unwrap(), Some(ReconciledStatus::Filled));
    assert_eq!(router.pending_unknown_count().unwrap(), 0);
    assert_eq!(adapter.submitted_count(), 1, "reconciliation never resubmits");
    assert_eq!(adapter.queried_count(), 1);
    assert!(audit
        .execution_records()
        .iter()
        .any(|event| event.phase == "live_submit_reconciled" && event.allowed));
}

#[test]
fn reconcile_unknown_marks_not_found_only_after_attempt_budget() {
    let (mut router, _audit) = make_router();
    let adapter = Arc::new(MockLiveAdapter::timeout_then_not_found());
    router.register_adapter(VenueId::new("test").unwrap(), adapter.clone());
    let intent = make_intent(false);
    let mut ctx = make_risk_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);

    assert_eq!(
        submit_authorized(&router, intent.clone(), &make_book(), &ctx),
        Err(RouterError::SubmitUnknown)
    );
    assert!(router.reconcile_unknowns(ctx.evaluated_at).unwrap().is_empty());
    assert_eq!(router.reconciliation_status(&intent.id).unwrap(), Some(ReconciledStatus::Unknown));
    assert!(router.reconcile_unknowns(ctx.evaluated_at).unwrap().is_empty());
    assert_eq!(router.reconciliation_status(&intent.id).unwrap(), Some(ReconciledStatus::Unknown));

    let updates = router.reconcile_unknowns(ctx.evaluated_at).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].status, ReconciledStatus::NotFound);
    assert_eq!(updates[0].attempts, 3);
    assert_eq!(adapter.submitted_count(), 1, "not-found budget never causes resubmit");
    assert_eq!(adapter.queried_count(), 3);
}

#[test]
fn audit_failure_prevents_execution() {
    let router = OrderRouter::new(test_router_config(), Arc::new(FailingAudit));
    let result = submit_authorized(&router, make_intent(true), &make_book(), &make_risk_context());
    assert_eq!(result, Err(RouterError::AuditUnavailable));
    assert_eq!(router.paper_counts().unwrap(), (0, 0));
}
