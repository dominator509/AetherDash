use crate::auth;
use aether_core::{
    MarketKey, OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce, Ulid,
    UtcTime,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Client-submitted order intent body (canonical shape, minus origin) ──

fn default_paper() -> bool {
    true
}

/// The client-submitted payload for an order intent. Matches the canonical
/// `aether_core::OrderIntent` minus the `origin` field (which is stamped
/// server-side from the authenticated session). Every field uses the canonical
/// domain type so invalid enums, malformed market keys, and bad decimal formats
/// are rejected during deserialization or validation.
#[derive(Debug, Deserialize)]
struct ClientOrderIntentBody {
    /// Optional client-suggested intent ID. Server generates one if missing.
    #[serde(default)]
    id: Option<String>,
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
    /// Quote snapshot at intent-creation time. Required for the canonical
    /// OrderIntent; the gateway accepts it from the client (stub — EP-401
    /// will fetch from the market data service).
    #[serde(default)]
    quote_snapshot: Option<aether_core::Quote>,
    /// Capability-set version. Client-supplied for now; EP-401 sources from DB.
    #[serde(default)]
    caps_version: Option<String>,
    /// Creation timestamp (RFC3339). Client-supplied for now; EP-401 stamps
    /// server-side.
    #[serde(default)]
    created_ts: Option<String>,
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
    /// Validate decimal strings and semantic rules, producing validated fields
    /// for constructing a canonical `aether_core::OrderIntent`.
    /// Error messages never echo client-controlled values per SPEC-006.
    fn validate(self) -> Result<ValidatedIntentFields, String> {
        let id = match self.id {
            Some(ref s) if !s.is_empty() => {
                Ulid::from_string(s).map_err(|_| "intent id must be a valid ULID".to_string())?
            }
            _ => Ulid::new(),
        };

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

        // Stub quote_snapshot: accept from client or use a minimal placeholder.
        // EP-401: the gateway will fetch a real quote from the market data service.
        let quote_snapshot = self.quote_snapshot.unwrap_or_else(|| aether_core::Quote {
            market: self.market.clone(),
            bid: None,
            ask: None,
            mid: None,
            last: None,
            bid_size: None,
            ask_size: None,
            ts: UtcTime::now(),
            source: aether_core::QuoteSource::Snapshot,
            seq: None,
        });

        let caps_version = match self.caps_version {
            Some(ref s) if !s.is_empty() => {
                Ulid::from_string(s).map_err(|_| "caps_version must be a valid ULID".to_string())?
            }
            _ => Ulid::new(),
        };

        let created_ts = match self.created_ts {
            Some(ref s) if !s.is_empty() => serde_json::from_str::<UtcTime>(&format!("\"{s}\""))
                .map_err(|_| "created_ts must be an RFC3339 UTC timestamp".to_string())?,
            _ => UtcTime::now(),
        };

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
            quote_snapshot,
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
fn session_origin_kind(kind: &str) -> Result<OriginKind, String> {
    match kind {
        "user" => Ok(OriginKind::User),
        "alert_action" => Ok(OriginKind::AlertAction),
        "agent" => Ok(OriginKind::Agent),
        "automation" => Ok(OriginKind::Automation),
        other => Err(format!("unknown session origin kind: {other}")),
    }
}

/// Extract the canonical actor ULID from the authenticated session.
/// Authentication state must contain a valid ULID; generating an unrelated
/// ephemeral identity would break audit continuity. Reject the request if
/// neither actor field is a valid ULID.
fn parse_session_actor(session: &auth::SessionInfo) -> Result<Ulid, String> {
    Ulid::from_string(&session.origin.actor_id)
        .or_else(|_| Ulid::from_string(&session.actor_id))
        .map_err(|_| "authenticated session has no valid actor ULID".to_string())
}

/// Dispatch a client frame to its server-frame response.
/// Stub: all channels accepted, commands echoed (real dispatch in EP-201/EP-305).
pub fn dispatch(frame: ClientFrame, session: &auth::SessionInfo) -> ServerFrame {
    match frame {
        ClientFrame::Subscribe { id, trace_id, channels } => {
            let trace_id = make_trace_id(&id, &trace_id);
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
            let mut body = serde_json::json!({
                "echo": text,
                "note": "MCP stub — command echo only",
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

            // Phase 1: deserialize into domain types. Use a fixed safe message
            // because serde errors for unknown enum variants can echo the
            // rejected value (SPEC-006).
            let parsed: ClientOrderIntentBody = match serde_json::from_value(body) {
                Ok(p) => p,
                Err(_e) => {
                    tracing::debug!(error = %_e, "order_intent deserialization failed");
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

            // Phase 2: validate decimals and semantic rules.
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

            // Phase 4: extract the authenticated actor ULID. If the session
            // does not carry a canonical actor ULID, reject the request.
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

            // Phase 5: stamp the canonical Origin from the authenticated session.
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

            // Phase 6: construct the canonical aether_core::OrderIntent with
            // the stamped trusted Origin. This is the single canonical intent
            // type defined by SPEC-003 — not a gateway-local partial shape.
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

            let tier_reason = if !intent.paper {
                format!(
                    "EP-401: live order requires tier-5 live authorization (caller tier {}) — not executed",
                    session.tier
                )
            } else {
                format!("EP-401: tier {} paper trade — allow-with-note", session.tier)
            };

            ServerFrame::ConfirmRequired {
                id,
                trace_id,
                ref_id: uuid::Uuid::new_v4().to_string(),
                action_summary,
                tier_reason,
                actor_id: intent.origin.actor_id.to_string(),
                origin_kind: session.origin.kind.clone(),
            }
        }
        ClientFrame::Confirm { id, trace_id, ref_id, totp } => {
            let trace_id = make_trace_id(&id, &trace_id);
            let totp_note = totp.as_ref().map(|_| "totp-provided").unwrap_or("totp-missing");
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "status": "confirmed",
                    "ref_id": ref_id,
                    "note": format!("stub — order not actually executed ({totp_note})"),
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

    /// A valid minimal OrderIntent body payload — uses proper domain types
    /// matching the canonical aether_core::OrderIntent minus origin.
    const VALID_INTENT_BODY: &str = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00"}"#;

    fn test_session() -> auth::SessionInfo {
        auth::SessionInfo {
            actor_id: ACTOR_ALICE.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: ACTOR_ALICE.into() },
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
    fn confirm_returns_command_result() {
        let conf = r#"{"type":"confirm","ref_id":"abc","totp":"123456"}"#;
        let frame: ClientFrame = serde_json::from_str(conf).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("command_result"));
        assert!(json.contains("confirmed"));
        assert!(json.contains("totp-provided"));
    }

    #[test]
    fn confirm_without_totp_notes_missing() {
        let conf = r#"{"type":"confirm","ref_id":"abc"}"#;
        let frame: ClientFrame = serde_json::from_str(conf).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("totp-missing"));
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
            (r#"{"type":"confirm","ref_id":"r1"}"#, "command_result"),
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
            actor_id: ACTOR_BOB.into(),
            tier: 1,
            origin: auth::OriginInfo { kind: "automation".into(), actor_id: ACTOR_BOB.into() },
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
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"sell","order_type":"market","size":"1.5","size_unit":"base","tif":"ioc","paper":true}"#;
        let oi = make_intent(body);
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
        assert!(json.contains("\"origin_kind\":\"user\""), "origin_kind should be session origin");
    }

    #[test]
    fn order_intent_invalid_body_type_returns_error() {
        let oi = r#"{"type":"order_intent","body":42}"#;
        let frame: ClientFrame = serde_json::from_str(oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid body type should produce error: {json}");
        assert!(json.contains("invalid_argument"), "should use invalid_argument: {json}");
        // Error message must not echo raw client input (SPEC-006)
        assert!(
            !json.contains("invalid order_intent:"),
            "error must not echo serde detail: {json}"
        );
    }

    #[test]
    fn order_intent_origin_never_from_client() {
        let session = auth::SessionInfo {
            actor_id: ACTOR_SYSTEM.into(),
            tier: 4,
            origin: auth::OriginInfo { kind: "agent".into(), actor_id: ACTOR_SYSTEM.into() },
        };
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"sell","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"50000.00","extra_field":"ignored"}"#;
        let oi = make_intent(body);
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
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"), "valid intent should succeed: {json}");
        assert!(json.contains("paper=true"), "omitted paper must default to true: {json}");
    }

    #[test]
    fn intent_explicit_paper_false_accepted_with_warning() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","paper":false}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains("confirm_required"),
            "explicit paper=false should be accepted (stub): {json}"
        );
        assert!(json.contains("paper=false"), "explicit paper=false must be honored: {json}");
        // Must carry explicit "requires live authorization" notice
        assert!(
            json.contains("live order requires tier-5 live authorization"),
            "paper=false must document live auth requirement: {json}"
        );
    }

    // ── Authenticated actor continuity ───────────────────────────────

    #[test]
    fn intent_stamps_exact_authenticated_actor() {
        let session = auth::SessionInfo {
            actor_id: ACTOR_BOB.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: ACTOR_BOB.into() },
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        // The actor_id in the response must be the exact authenticated ULID.
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_BOB}\"")),
            "must stamp exact authenticated actor ULID: {json}"
        );
    }

    #[test]
    fn intent_rejects_invalid_actor_ulid() {
        // Session with a non-ULID actor_id must be rejected.
        let session = auth::SessionInfo {
            actor_id: "not-a-valid-ulid".into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: "not-a-valid-ulid".into() },
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
        // Serde errors for unknown enum variants can include the rejected value.
        // The error response must use a fixed safe message.
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"long","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid side must error: {json}");
        // Must NOT echo "long" (the rejected input) in the message
        let msg_start = json.find("\"message\":\"").unwrap();
        let msg_end = json[msg_start..].find("\",\"").unwrap() + msg_start + 1;
        let message = &json[msg_start..msg_end];
        assert!(
            !message.contains("long"),
            "error message must not echo rejected enum value: {message}"
        );
    }

    // ── Adversarial: deserialization ─────────────────────────────────

    #[test]
    fn intent_missing_required_field_is_error() {
        let body = r#"{"side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "missing required field must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_market_key_is_error() {
        let body = r#"{"market":"BTC-USD","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid market key must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_side_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"long","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid side must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_size_unit_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"ounces","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid size_unit must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_malformed_decimal_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"not-a-number","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "malformed decimal must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
        // Error message must not echo the raw input per SPEC-006
        assert!(!json.contains("not-a-number"), "error must not echo raw input");
    }

    #[test]
    fn intent_invalid_tif_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"fok"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid tif must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_invalid_order_type_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"stop","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid order_type must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
    }

    #[test]
    fn intent_client_supplied_origin_data_is_ignored() {
        let session = auth::SessionInfo {
            actor_id: ACTOR_ALICE.into(),
            tier: 2,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: ACTOR_ALICE.into() },
        };
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","origin_kind":"attacker","actor_id":"evil"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("attacker"), "client origin_kind must not leak: {json}");
        assert!(!json.contains("evil"), "client actor_id must not leak: {json}");
        assert!(json.contains("\"origin_kind\":\"user\""), "origin from session: {json}");
        assert!(
            json.contains(&format!("\"actor_id\":\"{ACTOR_ALICE}\"")),
            "must contain exact authenticated ULID: {json}"
        );
    }

    // ── Adversarial: semantic validation ─────────────────────────────

    #[test]
    fn intent_size_zero_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "zero size must error: {json}");
    }

    #[test]
    fn intent_size_negative_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"-1.5","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "negative size must error: {json}");
    }

    #[test]
    fn intent_limit_without_price_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "limit order without price must error: {json}");
    }

    #[test]
    fn intent_market_with_price_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"market","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "market order with limit_price must error: {json}");
    }

    #[test]
    fn intent_market_order_without_price_accepted() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"market","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
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
            actor_id: ACTOR_ALICE.into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "superuser".into(), actor_id: ACTOR_ALICE.into() },
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
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","limit_price":"xyz","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "malformed limit_price must error: {json}");
        assert!(json.contains("invalid_argument"), "got: {json}");
        assert!(!json.contains("xyz"), "error must not echo raw input: {json}");
    }

    #[test]
    fn intent_empty_size_is_error() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "empty size must error: {json}");
    }

    // ── Canonical OrderIntent field coverage ─────────────────────────

    #[test]
    fn intent_accepts_full_canonical_shape() {
        // All canonical OrderIntent fields except origin, as SPEC-003 defines.
        let body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","limit_price":"65000.00","size":"0.01","size_unit":"contracts","tif":"gtc","paper":true,"quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(
            json.contains("confirm_required"),
            "full canonical intent shape must be accepted: {json}"
        );
        // The stamped actor_id must be the exact authenticated ULID
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
                origin_kind: "user".into(),
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
