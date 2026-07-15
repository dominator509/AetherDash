use crate::auth;
use aether_authz::{
    enforce_at, Action, Actor, ActorKind, AuditRecord, AuditSink, EnforcementPoint,
    EvaluationContext, Grant, Tier, Verdict,
};
use aether_core::{
    MarketKey, OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce, Ulid,
    UtcTime,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::collections::HashSet;

const CONFIRMATION_VALIDITY_SECS: u64 = 5 * 60;

#[derive(Debug, Clone)]
struct PendingConfirmation {
    actor_id: String,
    action: Action,
    expires_at: u64,
}

/// Connection-local confirmation state. References are never valid in a
/// different authenticated connection and are consumed exactly once.
#[derive(Debug, Default)]
pub struct ConnectionAuthState {
    pending: HashMap<String, PendingConfirmation>,
}

// ── Client-submitted order intent body (canonical shape, minus origin) ──

fn default_paper() -> bool {
    true
}

/// The client-submitted payload for an order intent. Matches the canonical
/// `aether_core::OrderIntent` minus the `origin` field (which is stamped
/// server-side from the authenticated session).
///
/// Every canonical field except origin is REQUIRED. Missing fields are
/// rejected during deserialization. Silent fabrication of provenance data
/// (intent id, quote snapshot, caps version, timestamps) is forbidden —
/// these are trust-boundary values that must be explicitly provided or
/// sourced from authoritative server state by EP-401.
#[derive(Debug, Deserialize)]
struct ClientOrderIntentBody {
    id: String,
    market: MarketKey,
    side: Side,
    order_type: OrderType,
    #[serde(default)]
    limit_price: Option<String>,
    size: String,
    size_unit: SizeUnit,
    tif: TimeInForce,
    #[serde(default = "default_paper")]
    paper: bool,
    /// Quote snapshot at intent-creation time. Must be for the same market.
    quote_snapshot: aether_core::Quote,
    /// Capability-set version in effect for this intent.
    caps_version: String,
    /// Creation timestamp (RFC3339 UTC).
    created_ts: String,
}

/// Client-validated fields, ready for origin stamping into a canonical OrderIntent.
struct ValidatedIntentFields {
    id: Ulid,
    market: MarketKey,
    side: Side,
    order_type: OrderType,
    limit_price: Option<Decimal>,
    size: Decimal,
    size_unit: SizeUnit,
    tif: TimeInForce,
    paper: bool,
    quote_snapshot: aether_core::Quote,
    caps_version: Ulid,
    created_ts: UtcTime,
}

impl ClientOrderIntentBody {
    /// Validate all fields and produce validated fields for constructing
    /// a canonical `aether_core::OrderIntent`.
    /// Error messages never echo client-controlled values per SPEC-006.
    fn validate(self) -> Result<ValidatedIntentFields, String> {
        let id = Ulid::from_string(&self.id)
            .map_err(|_| "intent id must be a valid ULID".to_string())?;

        let size = Decimal::from_str_exact(&self.size)
            .map_err(|_| "size must be a decimal string".to_string())?;
        if size <= Decimal::ZERO {
            return Err("size must be positive".to_string());
        }

        let limit_price = self
            .limit_price
            .filter(|s| !s.is_empty())
            .map(|_s| {
                Decimal::from_str_exact(&_s)
                    .map_err(|_| "limit_price must be a decimal string".to_string())
            })
            .transpose()?;

        match self.order_type {
            OrderType::Limit => {
                if limit_price.is_none() {
                    return Err("limit orders require limit_price".to_string());
                }
            }
            OrderType::Market => {
                if limit_price.is_some() {
                    return Err("market orders must not specify limit_price".to_string());
                }
            }
        }

        // Trust-boundary check: the quote snapshot must match the intent market.
        // A client cannot submit an intent for one market with a quote from another.
        if self.quote_snapshot.market != self.market {
            return Err("quote_snapshot market does not match intent market".to_string());
        }

        let caps_version = Ulid::from_string(&self.caps_version)
            .map_err(|_| "caps_version must be a valid ULID".to_string())?;

        let created_ts = serde_json::from_str::<UtcTime>(&format!("\"{}\"", self.created_ts))
            .map_err(|_| "created_ts must be an RFC3339 UTC timestamp".to_string())?;

        Ok(ValidatedIntentFields {
            id,
            market: self.market,
            side: self.side,
            order_type: self.order_type,
            limit_price,
            size,
            size_unit: self.size_unit,
            tif: self.tif,
            paper: self.paper,
            quote_snapshot: self.quote_snapshot,
            caps_version,
            created_ts,
        })
    }
}

// ── Client → Server frames ──
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientFrame {
    #[serde(rename = "subscribe")]
    Subscribe { id: Option<String>, trace_id: Option<String>, channels: Vec<String> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { id: Option<String>, trace_id: Option<String> },
    #[serde(rename = "command")]
    Command {
        id: Option<String>,
        trace_id: Option<String>,
        text: String,
        room_context: Option<String>,
    },
    #[serde(rename = "order_intent")]
    OrderIntent { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "confirm")]
    Confirm { id: Option<String>, trace_id: Option<String>, ref_id: String, totp: Option<String> },
    #[serde(rename = "ping")]
    Ping { id: Option<String>, trace_id: Option<String> },
}

// ── Server → Client frames ──
// Several variants (FeedItem, Quote, OrderUpdate, Alert, Explain, Degradation)
// are protocol contract definitions that will be constructed by EP-201/EP-305.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerFrame {
    #[serde(rename = "feed_item")]
    FeedItem { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "quote")]
    Quote { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "order_update")]
    OrderUpdate { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "alert")]
    Alert { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "explain")]
    Explain { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "command_result")]
    CommandResult { id: Option<String>, trace_id: Option<String>, body: serde_json::Value },
    #[serde(rename = "confirm_required")]
    ConfirmRequired {
        id: Option<String>,
        trace_id: Option<String>,
        ref_id: String,
        action_summary: String,
        tier_reason: String,
        actor_id: String,
        origin_kind: String,
    },
    #[serde(rename = "degradation")]
    Degradation { id: Option<String>, surface: String, reason: String },
    #[serde(rename = "error")]
    Error { id: Option<String>, trace_id: Option<String>, body: aether_core::error::ErrorEnvelope },
    #[serde(rename = "pong")]
    Pong { id: Option<String>, trace_id: Option<String> },
}

/// Generate a trace_id: use the client-supplied `trace_id` if present,
/// fall back to `id`, otherwise create a new UUID v4.
fn make_trace_id(client_id: &Option<String>, client_trace_id: &Option<String>) -> Option<String> {
    Some(
        client_trace_id
            .clone()
            .or_else(|| client_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    )
}

/// Map the session's string origin kind to the canonical aether_core OriginKind.
/// Returns an error for unknown kinds — never silently reclassifies.
/// Canonical values per SPEC-005: human, agent, automation.
fn session_origin_kind(kind: &str) -> Result<OriginKind, String> {
    match kind {
        "human" => Ok(OriginKind::Human),
        "agent" => Ok(OriginKind::Agent),
        "automation" => Ok(OriginKind::Automation),
        other => Err(format!("unknown session origin kind: {other}")),
    }
}

/// Extract the canonical actor ULID from the authenticated session.
/// Authentication state must contain a valid ULID; generating an unrelated
/// ephemeral identity would break audit continuity.
fn parse_session_actor(session: &auth::SessionInfo) -> Result<Ulid, String> {
    Ulid::from_string(&session.origin.actor_id)
        .or_else(|_| Ulid::from_string(&session.actor_id))
        .map_err(|_| "authenticated session has no valid actor ULID".to_string())
}

struct GatewayAuditSink;

impl AuditSink for GatewayAuditSink {
    type Error = Infallible;

    fn emit(&self, record: &AuditRecord) -> Result<(), Self::Error> {
        tracing::info!(
            target: "audit.events",
            actor_id = %record.actor_id,
            actor_kind = ?record.actor_kind,
            action = %record.action.scope(),
            grant_id = record.grant_id.as_deref().unwrap_or("none"),
            verdict = ?record.verdict,
            deciding_rule = record.deciding_rule,
            enforcement_point = ?record.enforcement_point,
            "authorization decision"
        );
        Ok(())
    }
}

fn authorize(
    session: &auth::SessionInfo,
    action: Action,
    confirmed: bool,
    step_up: bool,
) -> aether_authz::Decision {
    let actor_kind = match session.origin.kind.as_str() {
        "human" => ActorKind::Human,
        "agent" => ActorKind::Agent,
        "automation" => ActorKind::Automation,
        _ => {
            return aether_authz::Decision {
                verdict: Verdict::Deny,
                deciding_rule: "actor.unknown_kind",
                effective_tier: None,
                grant_id: None,
            };
        }
    };
    let tier = match Tier::try_from(session.tier) {
        Ok(value) => value,
        Err(_) => {
            return aether_authz::Decision {
                verdict: Verdict::Deny,
                deciding_rule: "tier.invalid",
                effective_tier: None,
                grant_id: None,
            };
        }
    };
    let actor = Actor { id: session.actor_id.clone(), kind: actor_kind };
    // validate_token already resolves the live DB grant and stores the lower
    // effective tier in SessionInfo. This request-local value is never cached.
    let grant = Grant {
        id: session.grant_id.clone(),
        actor_id: session.actor_id.clone(),
        actor_kind,
        tier,
        scopes: session.scopes.clone(),
        scope_restricted: session.scope_restricted,
        expires_at: None,
        revoked_at: None,
    };
    let now = unix_now();
    let mut context = EvaluationContext::new(now, Some(&grant));
    context.session_tier = (actor_kind == ActorKind::Human).then_some(tier);
    context.confirmed = confirmed;
    context.step_up_satisfied = step_up;
    enforce_at(EnforcementPoint::Gateway, &actor, action, context, &GatewayAuditSink)
        .map_or_else(|never| match never {}, |audited| audited.decision)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_secs())
}

fn permission_error(
    id: Option<String>,
    trace_id: Option<String>,
    decision: &aether_authz::Decision,
) -> ServerFrame {
    let (code, message) = match decision.verdict {
        Verdict::StepUpRequired => {
            (aether_core::ErrorCode::FailedPrecondition, "fresh step-up authentication is required")
        }
        _ => (aether_core::ErrorCode::PermissionDenied, "permission denied"),
    };
    ServerFrame::Error {
        id,
        trace_id,
        body: aether_core::ErrorEnvelope::new(code, message, Ulid::new())
            .with_details(format!("rule={}", decision.deciding_rule)),
    }
}

/// Convenience dispatch for isolated frames and unit tests. Stateful WebSocket
/// connections must use [`dispatch_with_state`] so confirmations can be bound
/// to the connection that issued them.
pub fn dispatch(frame: ClientFrame, session: &auth::SessionInfo) -> ServerFrame {
    dispatch_with_state(frame, session, &mut ConnectionAuthState::default())
}

/// Dispatch a client frame with connection-scoped, single-use confirmation state.
pub fn dispatch_with_state(
    frame: ClientFrame,
    session: &auth::SessionInfo,
    auth_state: &mut ConnectionAuthState,
) -> ServerFrame {
    match frame {
        ClientFrame::Subscribe { id, trace_id, channels } => {
            let trace_id = make_trace_id(&id, &trace_id);
            let decision = authorize(session, Action::Subscribe, false, false);
            if decision.verdict != Verdict::Allow {
                return permission_error(id, trace_id, &decision);
            }
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "status": "subscribed",
                    "channels": channels,
                    "actor_id": session.actor_id,
                    "origin_kind": session.origin.kind,
                }),
            }
        }
        ClientFrame::Unsubscribe { id, trace_id } => {
            let trace_id = make_trace_id(&id, &trace_id);
            let decision = authorize(session, Action::Subscribe, false, false);
            if decision.verdict != Verdict::Allow {
                return permission_error(id, trace_id, &decision);
            }
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "status": "unsubscribed",
                    "actor_id": session.actor_id,
                    "origin_kind": session.origin.kind,
                }),
            }
        }
        ClientFrame::Command { id, trace_id, text, room_context } => {
            let trace_id = make_trace_id(&id, &trace_id);
            let decision = authorize(session, Action::Query, false, false);
            if decision.verdict != Verdict::Allow {
                return permission_error(id, trace_id, &decision);
            }
            let mut body = serde_json::json!({
                "echo": text,
                "status": "authorized_command_echo",
                "actor_id": session.actor_id,
                "origin_kind": session.origin.kind,
            });
            if let Some(ref rc) = room_context {
                body["room_context"] = serde_json::Value::String(rc.clone());
            }
            ServerFrame::CommandResult { id, trace_id, body }
        }
        ClientFrame::OrderIntent { id, trace_id, body } => {
            let trace_id = make_trace_id(&id, &trace_id);

            // Phase 1: deserialize into domain types. Log only a fixed event —
            // serde error text can contain rejected client-controlled values.
            let parsed: ClientOrderIntentBody = match serde_json::from_value(body) {
                Ok(p) => p,
                Err(_) => {
                    tracing::debug!("order_intent deserialization failed");
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::InvalidArgument,
                            "order intent contains invalid or missing fields",
                            Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 2: validate decimals, semantics, and trust-boundary rules.
            let validated = match parsed.validate() {
                Ok(v) => v,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::InvalidArgument,
                            e,
                            Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 3: resolve the session origin kind.
            let origin_kind = match session_origin_kind(&session.origin.kind) {
                Ok(k) => k,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::PermissionDenied,
                            e,
                            Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 4: extract the authenticated actor ULID.
            let actor_ulid = match parse_session_actor(session) {
                Ok(ulid) => ulid,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::Unauthenticated,
                            e,
                            Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 5: stamp the canonical Origin.
            let origin = match Origin::new(origin_kind, session.tier, actor_ulid) {
                Ok(o) => o,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::InvalidArgument,
                            format!("invalid session origin: {e}"),
                            Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 6: authorization. The order router will independently repeat
            // this check in EP-305; gateway approval is never sufficient alone.
            let action =
                if validated.paper { Action::SubmitPaperOrder } else { Action::SubmitLiveOrder };
            let decision = authorize(session, action, false, false);
            if matches!(decision.verdict, Verdict::Deny | Verdict::StepUpRequired) {
                return permission_error(id, trace_id, &decision);
            }

            // ADR-0007 remains an out-of-band gate. EP-401 cannot and does not
            // flip it; live intents fail closed until EP-305 reads that gate.
            if !validated.paper {
                return ServerFrame::Error {
                    id,
                    trace_id,
                    body: aether_core::ErrorEnvelope::new(
                        aether_core::ErrorCode::FailedPrecondition,
                        "live order routing is disabled until the EP-305 execution gate",
                        Ulid::new(),
                    ),
                };
            }

            // Phase 7: construct the canonical aether_core::OrderIntent with
            // the stamped trusted Origin. All provenance fields come from the
            // validated client payload; none are silently fabricated.
            let intent = OrderIntent {
                id: validated.id,
                market: validated.market,
                side: validated.side,
                order_type: validated.order_type,
                limit_price: validated.limit_price,
                size: validated.size,
                size_unit: validated.size_unit,
                tif: validated.tif,
                paper: validated.paper,
                origin,
                quote_snapshot: validated.quote_snapshot,
                caps_version: validated.caps_version,
                created_ts: validated.created_ts,
            };

            let limit_str =
                intent.limit_price.as_ref().map(|p| format!(" @{p}")).unwrap_or_default();
            let action_summary = format!(
                "{order_type:?} {side:?} {size}{limit_str} {size_unit:?} {market} {tif:?} (paper={paper}) [id={id}]",
                order_type = intent.order_type,
                side = intent.side,
                size = intent.size,
                limit_str = limit_str,
                size_unit = intent.size_unit,
                market = intent.market,
                tif = intent.tif,
                paper = intent.paper,
                id = intent.id,
            );

            match decision.verdict {
                Verdict::ConfirmRequired => ServerFrame::ConfirmRequired {
                    id,
                    trace_id,
                    ref_id: {
                        let reference = uuid::Uuid::new_v4().to_string();
                        auth_state.pending.insert(
                            reference.clone(),
                            PendingConfirmation {
                                actor_id: session.actor_id.clone(),
                                action,
                                expires_at: unix_now().saturating_add(CONFIRMATION_VALIDITY_SECS),
                            },
                        );
                        reference
                    },
                    action_summary,
                    tier_reason: format!(
                        "tier {} requires confirmation for this mutation",
                        session.tier
                    ),
                    actor_id: intent.origin.actor_id.to_string(),
                    origin_kind: session.origin.kind.clone(),
                },
                Verdict::Allow => ServerFrame::CommandResult {
                    id,
                    trace_id,
                    body: serde_json::json!({
                        "status": "authorized",
                        "action": action.scope(),
                        "actor_id": intent.origin.actor_id.to_string(),
                        "origin_kind": session.origin.kind,
                    }),
                },
                Verdict::Deny | Verdict::StepUpRequired => {
                    permission_error(id, trace_id, &decision)
                }
            }
        }
        ClientFrame::Confirm { id, trace_id, ref_id, totp } => {
            let trace_id = make_trace_id(&id, &trace_id);
            // Never interpret mere TOTP presence as successful step-up. TOTP is
            // consumed only by aether-authz's verified challenge flow.
            let _ = totp;
            let Some(pending) = auth_state.pending.remove(&ref_id) else {
                return ServerFrame::Error {
                    id,
                    trace_id,
                    body: aether_core::ErrorEnvelope::new(
                        aether_core::ErrorCode::FailedPrecondition,
                        "confirmation reference is not active",
                        Ulid::new(),
                    ),
                };
            };
            if pending.actor_id != session.actor_id || unix_now() >= pending.expires_at {
                return ServerFrame::Error {
                    id,
                    trace_id,
                    body: aether_core::ErrorEnvelope::new(
                        aether_core::ErrorCode::FailedPrecondition,
                        "confirmation reference is stale or belongs to another actor",
                        Ulid::new(),
                    ),
                };
            }
            let decision = authorize(session, pending.action, true, false);
            if decision.verdict != Verdict::Allow {
                return permission_error(id, trace_id, &decision);
            }
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "status": "authorization_confirmed",
                    "action": pending.action.scope(),
                    "actor_id": session.actor_id,
                    "origin_kind": session.origin.kind,
                }),
            }
        }
        ClientFrame::Ping { id, trace_id } => {
            let trace_id = make_trace_id(&id, &trace_id);
            ServerFrame::Pong { id, trace_id }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid deterministic ULID for test actor identities.
    const ACTOR_ALICE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const ACTOR_BOB: &str = "01ARZ3NDEKTSV4RRFFQ69G5FBF";
    const ACTOR_SYSTEM: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCF";

    // Canonical required fields shared by all valid test intents.
    const PROVENANCE: &str = r#""id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z""#;

    /// Build a complete order_intent body with the given custom fields
    /// merged after the required provenance fields.
    fn intent_body(fields: &str) -> String {
        format!(r#"{{{PROVENANCE},{fields}}}"#)
    }

    /// Valid minimal OrderIntent body — all canonical fields present.
    const VALID_INTENT_BODY: &str = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;

    fn test_session() -> auth::SessionInfo {
        auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_ALICE.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "human".into(), actor_id: ACTOR_ALICE.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        }
    }

    fn make_intent(body: &str) -> String {
        format!(r#"{{"type":"order_intent","body":{body}}}"#)
    }

    // ── Basic frame tests ────────────────────────────────────────────

    #[test]
    fn ping_pong_round_trip() {
        let ping = r#"{"type":"ping","id":"1"}"#;
        let frame: ClientFrame = serde_json::from_str(ping).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"pong\""));
        assert!(json.contains("\"id\":\"1\""));
    }

    #[test]
    fn subscribe_returns_command_result() {
        let sub = r#"{"type":"subscribe","channels":["quotes:mkt:kalshi:BTC-75"]}"#;
        let frame: ClientFrame = serde_json::from_str(sub).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("command_result"));
        assert!(json.contains("subscribed"));
        assert!(json.contains("actor_id"));
    }

    #[test]
    fn unknown_type_is_error() {
        let unknown = r#"{"type":"bad_frame","x":1}"#;
        let result: Result<ClientFrame, _> = serde_json::from_str(unknown);
        assert!(result.is_err(), "unknown frame type must fail deserialization");
    }

    #[test]
    fn all_frame_types_round_trip() {
        let oi_json = make_intent(VALID_INTENT_BODY);
        let cases: [(&str, bool); 6] = [
            (r#"{"type":"subscribe","channels":[]}"#, true),
            (r#"{"type":"unsubscribe"}"#, true),
            (r#"{"type":"command","text":"help"}"#, true),
            (&oi_json, true),
            (r#"{"type":"confirm","ref_id":"abc","totp":null}"#, true),
            (r#"{"type":"ping"}"#, true),
        ];
        let mut all_ok = true;
        for (json, expect_ok) in &cases {
            let result: Result<ClientFrame, _> = serde_json::from_str(json);
            if result.is_ok() != *expect_ok {
                all_ok = false;
                eprintln!("failed for: {json}");
            }
        }
        assert!(all_ok, "all frame types must round-trip");
    }

    #[test]
    fn unsubscribe_returns_command_result() {
        let unsub = r#"{"type":"unsubscribe","id":"u1"}"#;
        let frame: ClientFrame = serde_json::from_str(unsub).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("command_result"));
        assert!(json.contains("unsubscribed"));
    }

    #[test]
    fn untracked_confirmation_reference_fails_closed() {
        let conf = r#"{"type":"confirm","ref_id":"abc","totp":"123456"}"#;
        let frame: ClientFrame = serde_json::from_str(conf).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("failed_precondition"));
        assert!(!json.contains("confirmed"));
    }

    #[test]
    fn confirmation_without_totp_fails_closed_without_echoing_credential_state() {
        let conf = r#"{"type":"confirm","ref_id":"abc"}"#;
        let frame: ClientFrame = serde_json::from_str(conf).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("failed_precondition"));
        assert!(!json.contains("totp"));
    }

    #[test]
    fn issued_confirmation_is_connection_bound_and_single_use() {
        let session = test_session();
        let mut state = ConnectionAuthState::default();
        let order: ClientFrame = serde_json::from_str(&make_intent(VALID_INTENT_BODY)).unwrap();
        let reference = match dispatch_with_state(order, &session, &mut state) {
            ServerFrame::ConfirmRequired { ref_id, .. } => ref_id,
            other => panic!(
                "expected confirmation challenge, got {}",
                serde_json::to_string(&other).unwrap()
            ),
        };

        let confirm: ClientFrame =
            serde_json::from_str(&format!(r#"{{"type":"confirm","ref_id":"{reference}"}}"#))
                .unwrap();
        let accepted = dispatch_with_state(confirm, &session, &mut state);
        let accepted_json = serde_json::to_string(&accepted).unwrap();
        assert!(accepted_json.contains("authorization_confirmed"));

        let replay: ClientFrame =
            serde_json::from_str(&format!(r#"{{"type":"confirm","ref_id":"{reference}"}}"#))
                .unwrap();
        let replayed = dispatch_with_state(replay, &session, &mut state);
        assert!(serde_json::to_string(&replayed).unwrap().contains("failed_precondition"));
    }

    #[test]
    fn gateway_enforces_grant_scope_before_issuing_confirmation() {
        let mut session = test_session();
        session.scope_restricted = true;
        session.scopes.insert("data.query".into());
        let order: ClientFrame = serde_json::from_str(&make_intent(VALID_INTENT_BODY)).unwrap();
        let denied = dispatch(order, &session);
        let json = serde_json::to_string(&denied).unwrap();
        assert!(json.contains("permission_denied"));
        assert!(json.contains("grant.scope_denied"));
    }

    #[test]
    fn command_room_context_reflected_in_response() {
        let cmd = r#"{"type":"command","text":"status","room_context":"war-room"}"#;
        let frame: ClientFrame = serde_json::from_str(cmd).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("war-room"), "room_context not reflected: {json}");
    }

    // ── OrderIntent — valid dispatch ─────────────────────────────────

    #[test]
    fn order_intent_returns_confirm_required() {
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"), "got: {json}");
        assert!(json.contains("ref_id"), "missing ref_id: {json}");
        assert!(json.contains("actor_id"), "missing actor_id: {json}");
    }

    #[test]
    fn trace_id_propagated_from_client_id() {
        let ping = r#"{"type":"ping","id":"my-trace-1"}"#;
        let frame: ClientFrame = serde_json::from_str(ping).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"id\":\"my-trace-1\""));
        assert!(json.contains("\"trace_id\":\"my-trace-1\""));
    }

    #[test]
    fn trace_id_distinct_from_client_id() {
        let ping = r#"{"type":"ping","id":"req-123","trace_id":"trace-456"}"#;
        let frame: ClientFrame = serde_json::from_str(ping).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"id\":\"req-123\""));
        assert!(json.contains("\"trace_id\":\"trace-456\""));
    }

    #[test]
    fn all_six_client_types_dispatch_to_correct_server_type() {
        let session = test_session();
        let oi = make_intent(VALID_INTENT_BODY);
        let cases: Vec<(&str, &str)> = vec![
            (r#"{"type":"subscribe","channels":["a"]}"#, "command_result"),
            (r#"{"type":"unsubscribe","id":"u1"}"#, "command_result"),
            (r#"{"type":"command","text":"hi"}"#, "command_result"),
            (oi.as_str(), "confirm_required"),
            (r#"{"type":"confirm","ref_id":"r1"}"#, "error"),
            (r#"{"type":"ping"}"#, "pong"),
        ];
        for (json, expected_type) in &cases {
            let frame: ClientFrame = serde_json::from_str(json).unwrap();
            let response = dispatch(frame, &session);
            let out = serde_json::to_string(&response).unwrap();
            assert!(
                out.contains(&format!("\"{}\"", expected_type)),
                "expected {expected_type} for {json}, got {out}"
            );
        }
    }

    #[test]
    fn session_origin_stamped_on_order_intent() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_BOB.into(),
            tier: 5,
            origin: auth::OriginInfo { kind: "automation".into(), actor_id: ACTOR_BOB.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_BOB}\"")),
            "must contain the exact authenticated actor ULID: {json}"
        );
        assert!(
            json.contains("\"origin_kind\":\"automation\""),
            "should contain origin_kind: {json}"
        );
    }

    #[test]
    fn trace_id_generated_when_client_id_missing() {
        let ping = r#"{"type":"ping"}"#;
        let frame: ClientFrame = serde_json::from_str(ping).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"trace_id\":"), "missing trace_id: {json}");
    }

    #[test]
    fn order_intent_body_fields_appear_in_response() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"sell","order_type":"market","size":"1.5","size_unit":"base","tif":"ioc","paper":true"#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"));
        assert!(json.contains("mkt:kalshi:BTC-75"), "missing market: {json}");
        assert!(json.contains("Sell"), "missing side: {json}");
        assert!(json.contains("Market"), "missing order_type: {json}");
        assert!(json.contains("1.5"), "missing size: {json}");
        assert!(json.contains("Base"), "missing size_unit: {json}");
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_ALICE}\"")),
            "must contain exact authenticated actor ULID: {json}"
        );
        assert!(json.contains("\"origin_kind\":\"human\""), "origin_kind should be session origin");
    }

    #[test]
    fn order_intent_invalid_body_type_returns_error() {
        let oi = r#"{"type":"order_intent","body":42}"#;
        let frame: ClientFrame = serde_json::from_str(oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid body type should produce error: {json}");
        assert!(json.contains("invalid_argument"), "should use invalid_argument: {json}");
    }

    #[test]
    fn order_intent_origin_never_from_client() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_SYSTEM.into(),
            tier: 4,
            origin: auth::OriginInfo { kind: "agent".into(), actor_id: ACTOR_SYSTEM.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"sell","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"50000.00","extra_field":"ignored""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"origin_kind\":\"agent\""), "origin_kind from session: {json}");
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_SYSTEM}\"")),
            "must contain exact authenticated actor ULID: {json}"
        );
        assert!(!json.contains("extra_field"), "unknown fields must not leak: {json}");
    }

    // ── Paper default ────────────────────────────────────────────────

    #[test]
    fn intent_paper_defaults_to_true() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"), "valid intent should succeed: {json}");
        assert!(json.contains("paper=true"), "omitted paper must default to true: {json}");
    }

    // ── Live-order authorization boundary ───────────────────────────

    #[test]
    fn intent_live_order_returns_failed_precondition() {
        // paper=false must return failed_precondition, not confirm_required
        // with a warning. A textual warning is not an authorization boundary.
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","paper":false"#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let mut session = test_session();
        session.tier = 4;
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "live order must return error before EP-401: {json}");
        assert!(json.contains("failed_precondition"), "must use failed_precondition, got: {json}");
        assert!(
            !json.contains("confirm_required"),
            "live order must not reach confirm_required: {json}"
        );
    }

    // ── Authenticated actor continuity ───────────────────────────────

    #[test]
    fn intent_stamps_exact_authenticated_actor() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_BOB.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "human".into(), actor_id: ACTOR_BOB.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_BOB}\"")),
            "must stamp exact authenticated actor ULID: {json}"
        );
    }

    #[test]
    fn intent_rejects_invalid_actor_ulid() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: "not-a-valid-ulid".into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "human".into(), actor_id: "not-a-valid-ulid".into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "non-ULID actor must be rejected: {json}");
        assert!(
            json.contains("unauthenticated"),
            "must use unauthenticated code for invalid actor: {json}"
        );
    }

    // ── Deserialization errors do not echo client input ──────────────

    #[test]
    fn intent_invalid_enum_value_does_not_leak_input() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"long","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid side must error: {json}");
        let msg_start = json.find("\"message\":\"").unwrap();
        let msg_end = json[msg_start..].find("\",\"").unwrap() + msg_start + 1;
        let message = &json[msg_start..msg_end];
        assert!(
            !message.contains("long"),
            "error message must not echo rejected enum value: {message}"
        );
    }

    // ── Trust-boundary: provenance not fabricated ─────────────────────

    #[test]
    fn intent_missing_id_is_error() {
        // id is a required canonical field — must not be fabricated.
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing id must error: {json}");
    }

    #[test]
    fn intent_missing_quote_snapshot_is_error() {
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing quote_snapshot must error: {json}");
    }

    #[test]
    fn intent_missing_caps_version_is_error() {
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing caps_version must error: {json}");
    }

    #[test]
    fn intent_missing_created_ts_is_error() {
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing created_ts must error: {json}");
    }

    #[test]
    fn intent_quote_market_mismatch_is_error() {
        // Quote snapshot for a different market must be rejected.
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:ETH-50","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "quote market mismatch must error: {json}");
        assert!(
            json.contains("quote_snapshot market does not match"),
            "specific error expected: {json}"
        );
    }

    // ── Adversarial: deserialization ─────────────────────────────────

    #[test]
    fn intent_missing_required_field_is_error() {
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing required field must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_market_key_is_error() {
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"BTC-USD","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid market key must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_side_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"long","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid side must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_size_unit_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"ounces","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid size_unit must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_malformed_decimal_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"not-a-number","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "malformed decimal must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
        assert!(!json.contains("not-a-number"), "error must not echo raw input");
    }

    #[test]
    fn intent_invalid_tif_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"fok""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid tif must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_order_type_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"stop","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid order_type must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_client_supplied_origin_data_is_ignored() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_ALICE.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "human".into(), actor_id: ACTOR_ALICE.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","origin_kind":"attacker","actor_id":"evil""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("attacker"), "client origin_kind must not leak: {json}");
        assert!(!json.contains("evil"), "client actor_id must not leak: {json}");
        assert!(json.contains("\"origin_kind\":\"human\""), "origin from session: {json}");
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_ALICE}\"")),
            "must contain exact authenticated ULID: {json}"
        );
    }

    // ── Adversarial: semantic validation ─────────────────────────────

    #[test]
    fn intent_size_zero_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "zero size must error: {json}");
    }

    #[test]
    fn intent_size_negative_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"-1.5","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "negative size must error: {json}");
    }

    #[test]
    fn intent_limit_without_price_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "limit order without price must error: {json}");
    }

    #[test]
    fn intent_market_with_price_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"market","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "market order with limit_price must error: {json}");
    }

    #[test]
    fn intent_market_order_without_price_accepted() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"market","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains("confirm_required"),
            "market order without limit_price must be accepted: {json}"
        );
    }

    #[test]
    fn intent_unknown_origin_kind_is_error() {
        let session = auth::SessionInfo {
            session_id: "test-session".into(),
            actor_id: ACTOR_ALICE.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "superuser".into(), actor_id: ACTOR_ALICE.into() },
            device_label: None,
            grant_id: "test-grant".into(),
            scopes: HashSet::new(),
            scope_restricted: false,
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "unknown origin kind must error: {json}");
        assert!(json.contains("permission_denied"), "should use permission_denied: {json}");
    }

    #[test]
    fn intent_limit_price_malformed_decimal_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","limit_price":"xyz","size":"0.01","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "malformed limit_price must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
        assert!(!json.contains("xyz"), "error must not echo raw input: {json}");
    }

    #[test]
    fn intent_empty_size_is_error() {
        let body = intent_body(
            r#""market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"","size_unit":"contracts","tif":"gtc""#,
        );
        let oi = make_intent(&body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "empty size must error: {json}");
    }

    // ── Canonical shape acceptance ────────────────────────────────────

    #[test]
    fn intent_accepts_full_canonical_shape() {
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains("confirm_required"),
            "full canonical intent shape must be accepted: {json}"
        );
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_ALICE}\"")),
            "must stamp exact actor: {json}"
        );
    }

    // ── Server frame coverage ────────────────────────────────────────

    #[test]
    fn all_server_frame_variants_constructible() {
        let frames: Vec<ServerFrame> = vec![
            ServerFrame::FeedItem { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::Quote { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::OrderUpdate { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::Alert { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::Explain { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::CommandResult { id: None, trace_id: None, body: serde_json::json!({}) },
            ServerFrame::ConfirmRequired {
                id: None,
                trace_id: None,
                ref_id: "r1".into(),
                action_summary: "test".into(),
                tier_reason: "test".into(),
                actor_id: "a".into(),
                origin_kind: "human".into(),
            },
            ServerFrame::Degradation { id: None, surface: "test".into(), reason: "test".into() },
            ServerFrame::Error {
                id: None,
                trace_id: None,
                body: aether_core::ErrorEnvelope::new(
                    aether_core::ErrorCode::Internal,
                    "test",
                    Ulid::new(),
                ),
            },
            ServerFrame::Pong { id: None, trace_id: None },
        ];
        assert_eq!(frames.len(), 10, "all 10 server frame variants must be constructible");
    }
}
