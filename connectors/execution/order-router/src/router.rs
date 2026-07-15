//! Fail-closed order routing: idempotency -> authorization -> risk -> paper ledger.

use crate::adapter::{AdapterError, AdapterRegistry, VenueAdapterClient, VenueOrderObservation};
use crate::breaker::RouterBreakers;
use crate::reconciler::{ReconciledStatus, Reconciler, VenueObservation};
use aether_authz::{
    enforce_at, Action, Actor, AuditRecord, AuditSink, EnforcementPoint, EvaluationContext, Verdict,
};
use aether_core::ids::{Ulid, VenueId};
use aether_core::order::{Fill, OrderIntent, OriginKind, RiskReason, RiskReasonCode, RiskVerdict};
use aether_core::quote::OrderBook;
use aether_core::time::UtcTime;
use aether_paper_ledger::ledger::PaperLedger;
use aether_risk_engine::engine::{RiskConfig, RiskContext, RiskEngine};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RouterError {
    #[error("risk check denied: {0:?}")]
    RiskDenied(Vec<RiskReason>),
    #[error("authorization denied: {0}")]
    Unauthorized(&'static str),
    #[error("audit sink unavailable")]
    AuditUnavailable,
    #[error("router state unavailable")]
    StateUnavailable,
    #[error("intent is already being processed")]
    InProgress,
    #[error("maximum concurrent order limit reached")]
    CapacityExceeded,
    #[error("order book market does not match intent")]
    MarketMismatch,
    #[error("paper ledger error: {0}")]
    PaperLedger(String),
    #[error("venue submit is not implemented; live execution remains unavailable")]
    VenueNotImplemented,
    #[error("venue submit timed out; order state is unknown until reconciled")]
    SubmitUnknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouterResult {
    pub verdict: RiskVerdict,
    pub fills: Vec<Fill>,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub risk_config: RiskConfig,
    pub max_concurrent_orders: usize,
    pub max_reconciliation_attempts: u32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            risk_config: RiskConfig::default(),
            max_concurrent_orders: 100,
            max_reconciliation_attempts: 3,
        }
    }
}

pub struct RouterAuthContext<'a> {
    pub actor: &'a Actor,
    pub evaluation: EvaluationContext<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAuditEvent {
    pub intent_id: Ulid,
    pub at: UtcTime,
    pub phase: &'static str,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationUpdate {
    pub intent_id: Ulid,
    pub status: ReconciledStatus,
    pub attempts: u32,
}

#[derive(Debug, Error)]
#[error("execution audit sink rejected the event")]
pub struct ExecutionAuditError;

pub trait ExecutionAuditSink: Send + Sync {
    fn emit_authz(&self, record: &AuditRecord) -> Result<(), ExecutionAuditError>;
    fn emit_execution(&self, event: &ExecutionAuditEvent) -> Result<(), ExecutionAuditError>;
}

#[derive(Default)]
pub struct MemoryExecutionAuditSink {
    authz: Mutex<Vec<AuditRecord>>,
    execution: Mutex<Vec<ExecutionAuditEvent>>,
}

impl MemoryExecutionAuditSink {
    pub fn authz_records(&self) -> Vec<AuditRecord> {
        self.authz.lock().map_or_else(|_| Vec::new(), |records| records.clone())
    }
    pub fn execution_records(&self) -> Vec<ExecutionAuditEvent> {
        self.execution.lock().map_or_else(|_| Vec::new(), |records| records.clone())
    }
}

impl ExecutionAuditSink for MemoryExecutionAuditSink {
    fn emit_authz(&self, record: &AuditRecord) -> Result<(), ExecutionAuditError> {
        self.authz.lock().map_err(|_| ExecutionAuditError)?.push(record.clone());
        Ok(())
    }
    fn emit_execution(&self, event: &ExecutionAuditEvent) -> Result<(), ExecutionAuditError> {
        self.execution.lock().map_err(|_| ExecutionAuditError)?.push(event.clone());
        Ok(())
    }
}

struct AuthzAuditAdapter<'a>(&'a dyn ExecutionAuditSink);
impl AuditSink for AuthzAuditAdapter<'_> {
    type Error = ExecutionAuditError;
    fn emit(&self, record: &AuditRecord) -> Result<(), Self::Error> {
        self.0.emit_authz(record)
    }
}

#[derive(Clone)]
enum IntentState {
    Processing,
    Terminal(Result<RouterResult, RouterError>),
}

pub struct OrderRouter {
    risk_engine: RiskEngine,
    paper_ledger: Mutex<PaperLedger>,
    intents: Mutex<HashMap<Ulid, IntentState>>,
    reconciler: Mutex<Reconciler>,
    audit: Arc<dyn ExecutionAuditSink>,
    breakers: RouterBreakers,
    adapter_registry: AdapterRegistry,
    #[allow(dead_code)]
    config: RouterConfig,
}

impl OrderRouter {
    pub fn new(config: RouterConfig, audit: Arc<dyn ExecutionAuditSink>) -> Self {
        Self {
            risk_engine: RiskEngine::new(config.risk_config.clone()),
            paper_ledger: Mutex::new(PaperLedger::new()),
            intents: Mutex::new(HashMap::new()),
            reconciler: Mutex::new(Reconciler::new()),
            audit,
            breakers: RouterBreakers::with_defaults(),
            adapter_registry: AdapterRegistry::default(),
            config,
        }
    }

    /// Register a venue adapter for live order dispatch.
    pub fn register_adapter(&mut self, venue_id: VenueId, adapter: Arc<dyn VenueAdapterClient>) {
        self.adapter_registry.register(venue_id, adapter);
    }

    /// Create a router with an explicit adapter registry.
    pub fn with_registry(
        config: RouterConfig,
        audit: Arc<dyn ExecutionAuditSink>,
        registry: AdapterRegistry,
    ) -> Self {
        Self {
            risk_engine: RiskEngine::new(config.risk_config.clone()),
            paper_ledger: Mutex::new(PaperLedger::new()),
            intents: Mutex::new(HashMap::new()),
            reconciler: Mutex::new(Reconciler::new()),
            audit,
            breakers: RouterBreakers::with_defaults(),
            adapter_registry: registry,
            config,
        }
    }

    pub fn submit(
        &self,
        intent: OrderIntent,
        book: &OrderBook,
        risk_context: &RiskContext,
        auth: &RouterAuthContext<'_>,
    ) -> Result<RouterResult, RouterError> {
        if let Some(previous) = self.reserve(intent.id)? {
            return previous.map(|mut result| {
                result.replayed = true;
                result
            });
        }

        let outcome = self.submit_reserved(&intent, book, risk_context, auth);
        self.finish(intent.id, outcome.clone())?;
        outcome
    }

    fn submit_reserved(
        &self,
        intent: &OrderIntent,
        book: &OrderBook,
        risk_context: &RiskContext,
        auth: &RouterAuthContext<'_>,
    ) -> Result<RouterResult, RouterError> {
        if book.market != intent.market {
            return Err(RouterError::MarketMismatch);
        }

        let action = if intent.paper { Action::SubmitPaperOrder } else { Action::SubmitLiveOrder };
        let adapter = AuthzAuditAdapter(self.audit.as_ref());
        let authorization =
            enforce_at(EnforcementPoint::Router, auth.actor, action, auth.evaluation, &adapter)
                .map_err(|_| RouterError::AuditUnavailable)?;
        if authorization.decision.verdict != Verdict::Allow {
            return Err(RouterError::Unauthorized(authorization.decision.deciding_rule));
        }
        let origin_kind = match intent.origin.kind {
            OriginKind::Human => aether_authz::ActorKind::Human,
            OriginKind::Agent => aether_authz::ActorKind::Agent,
            OriginKind::Automation => aether_authz::ActorKind::Automation,
        };
        if auth.actor.id != intent.origin.actor_id.to_string() || auth.actor.kind != origin_kind {
            self.audit
                .emit_execution(&ExecutionAuditEvent {
                    intent_id: intent.id,
                    at: risk_context.evaluated_at,
                    phase: "intent_origin_binding",
                    allowed: false,
                })
                .map_err(|_| RouterError::AuditUnavailable)?;
            return Err(RouterError::Unauthorized("intent.origin_mismatch"));
        }

        let verdict = self.risk_engine.evaluate(intent, risk_context);
        self.audit
            .emit_execution(&ExecutionAuditEvent {
                intent_id: intent.id,
                at: risk_context.evaluated_at,
                phase: "risk_verdict",
                allowed: verdict.is_allowed(),
            })
            .map_err(|_| RouterError::AuditUnavailable)?;
        if !verdict.is_allowed() {
            return Err(RouterError::RiskDenied(verdict.reasons));
        }

        if !intent.paper {
            // Live path: dispatch to the adapter for the authoritative
            // market venue from the risk snapshot. Do not parse MarketKey
            // strings here; the risk context already validated metadata.
            let venue_id = risk_context
                .markets
                .get(&intent.market)
                .map(|market| market.venue.clone())
                .ok_or(RouterError::StateUnavailable)?;

            if !self.breakers.is_allowed(&venue_id) {
                return Err(RouterError::RiskDenied(vec![RiskReason {
                    code: RiskReasonCode::VenueHealth,
                    detail: format!("circuit breaker is open for venue {}", venue_id.as_str()),
                }]));
            }

            let adapter = self.adapter_registry.get(&venue_id).ok_or_else(|| {
                self.breakers.record_failure(&venue_id);
                RouterError::VenueNotImplemented
            })?;

            match adapter.submit_order(intent) {
                Ok(result) => {
                    if !result.accepted {
                        self.breakers.record_failure(&venue_id);
                        return Err(RouterError::RiskDenied(vec![RiskReason {
                            code: RiskReasonCode::VenueHealth,
                            detail: format!("venue rejected: {}", result.status),
                        }]));
                    }
                    self.breakers.record_success(&venue_id);
                    self.audit
                        .emit_execution(&ExecutionAuditEvent {
                            intent_id: intent.id,
                            at: risk_context.evaluated_at,
                            phase: "live_submit_accepted",
                            allowed: true,
                        })
                        .map_err(|_| RouterError::AuditUnavailable)?;
                    // Live fills are produced by later venue observations.
                    return Ok(RouterResult { verdict, fills: Vec::new(), replayed: false });
                }
                Err(AdapterError::CapabilityMissing(_)) => {
                    self.breakers.record_failure(&venue_id);
                    return Err(RouterError::VenueNotImplemented);
                }
                Err(AdapterError::Timeout) => {
                    self.breakers.record_failure(&venue_id);
                    self.reconciler
                        .lock()
                        .map_err(|_| RouterError::StateUnavailable)?
                        .begin_unknown(intent.id, venue_id.clone(), intent.id.to_string());
                    self.audit
                        .emit_execution(&ExecutionAuditEvent {
                            intent_id: intent.id,
                            at: risk_context.evaluated_at,
                            phase: "live_submit_unknown",
                            allowed: false,
                        })
                        .map_err(|_| RouterError::AuditUnavailable)?;
                    return Err(RouterError::SubmitUnknown);
                }
                Err(e) => {
                    self.breakers.record_failure(&venue_id);
                    return Err(RouterError::RiskDenied(vec![RiskReason {
                        code: RiskReasonCode::VenueHealth,
                        detail: format!("venue adapter error: {}", e),
                    }]));
                }
            }
        }

        // Persist authorization to execute before mutating the paper ledger.
        self.audit
            .emit_execution(&ExecutionAuditEvent {
                intent_id: intent.id,
                at: risk_context.evaluated_at,
                phase: "paper_execution_authorized",
                allowed: true,
            })
            .map_err(|_| RouterError::AuditUnavailable)?;
        let fills = self
            .paper_ledger
            .lock()
            .map_err(|_| RouterError::StateUnavailable)?
            .submit(intent.clone(), book)
            .map_err(|error| RouterError::PaperLedger(error.to_string()))?;
        Ok(RouterResult { verdict, fills, replayed: false })
    }

    fn reserve(&self, id: Ulid) -> Result<Option<Result<RouterResult, RouterError>>, RouterError> {
        let mut intents = self.intents.lock().map_err(|_| RouterError::StateUnavailable)?;
        if let Some(state) = intents.get(&id) {
            return match state {
                IntentState::Processing => Err(RouterError::InProgress),
                IntentState::Terminal(outcome) => Ok(Some(outcome.clone())),
            };
        }
        let in_flight =
            intents.values().filter(|state| matches!(state, IntentState::Processing)).count();
        if in_flight >= self.config.max_concurrent_orders {
            return Err(RouterError::CapacityExceeded);
        }
        intents.insert(id, IntentState::Processing);
        Ok(None)
    }

    fn finish(
        &self,
        id: Ulid,
        outcome: Result<RouterResult, RouterError>,
    ) -> Result<(), RouterError> {
        self.intents
            .lock()
            .map_err(|_| RouterError::StateUnavailable)?
            .insert(id, IntentState::Terminal(outcome));
        Ok(())
    }

    pub fn is_seen(&self, id: &Ulid) -> bool {
        self.intents.lock().is_ok_and(|intents| intents.contains_key(id))
    }

    pub fn get_result(&self, id: &Ulid) -> Option<RouterResult> {
        self.intents.lock().ok().and_then(|intents| match intents.get(id) {
            Some(IntentState::Terminal(Ok(result))) => Some(result.clone()),
            _ => None,
        })
    }

    pub fn paper_counts(&self) -> Result<(usize, usize), RouterError> {
        let ledger = self.paper_ledger.lock().map_err(|_| RouterError::StateUnavailable)?;
        Ok((ledger.orders().len(), ledger.fills().len()))
    }

    pub fn reconciliation_status(
        &self,
        id: &Ulid,
    ) -> Result<Option<ReconciledStatus>, RouterError> {
        Ok(self.reconciler.lock().map_err(|_| RouterError::StateUnavailable)?.status(id))
    }

    pub fn pending_unknown_count(&self) -> Result<usize, RouterError> {
        Ok(self.reconciler.lock().map_err(|_| RouterError::StateUnavailable)?.pending_count())
    }

    pub fn reconcile_unknowns(
        &self,
        evaluated_at: UtcTime,
    ) -> Result<Vec<ReconciliationUpdate>, RouterError> {
        let targets =
            self.reconciler.lock().map_err(|_| RouterError::StateUnavailable)?.pending_targets();
        let mut updates = Vec::new();

        for target in targets {
            let Some(adapter) = self.adapter_registry.get(&target.venue_id) else {
                continue;
            };

            let attempts = self
                .reconciler
                .lock()
                .map_err(|_| RouterError::StateUnavailable)?
                .record_attempt(&target.order_id)
                .unwrap_or(target.attempts);

            let query_result = adapter.query_order(&target.client_order_id);
            let observation = match query_result {
                Ok(result) => Some(map_observation(result.observation())),
                Err(AdapterError::NotFound(_))
                    if attempts >= self.config.max_reconciliation_attempts =>
                {
                    Some(VenueObservation::NotFound)
                }
                Err(AdapterError::NotFound(_)) | Err(AdapterError::Timeout) => None,
                Err(_) => {
                    self.breakers.record_failure(&target.venue_id);
                    None
                }
            };

            if let Some(observation) = observation {
                let status = {
                    let mut reconciler =
                        self.reconciler.lock().map_err(|_| RouterError::StateUnavailable)?;
                    reconciler.observe(target.order_id, observation);
                    reconciler.status(&target.order_id)
                };
                if let Some(status) = status {
                    self.audit
                        .emit_execution(&ExecutionAuditEvent {
                            intent_id: target.order_id,
                            at: evaluated_at,
                            phase: "live_submit_reconciled",
                            allowed: status != ReconciledStatus::NotFound,
                        })
                        .map_err(|_| RouterError::AuditUnavailable)?;
                    updates.push(ReconciliationUpdate {
                        intent_id: target.order_id,
                        status,
                        attempts,
                    });
                }
            }
        }

        Ok(updates)
    }
}

fn map_observation(observation: VenueOrderObservation) -> VenueObservation {
    match observation {
        VenueOrderObservation::Filled => VenueObservation::Filled,
        VenueOrderObservation::Open => VenueObservation::Open,
        VenueOrderObservation::Cancelled => VenueObservation::Cancelled,
        VenueOrderObservation::Rejected => VenueObservation::Rejected,
    }
}
