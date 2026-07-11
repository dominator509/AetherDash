use crate::auth;
use aether_core::{MarketKey, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ── Client intent body (validated domain types) ──

/// Client-supplied order-intent payload. Every field uses the canonical
/// aether_core domain type so invalid enums, malformed market keys, and
/// bad decimal formats are rejected during deserialization or validation.
///
/// Origin is NEVER taken from the client body — it is always stamped from
/// the authenticated session after validation succeeds.
#[derive(Debug, Deserialize)]
struct ClientIntentBody {
    market: MarketKey,
    side: Side,
    order_type: OrderType,
    #[serde(default)]
    limit_price: Option<String>,
    size: String,
    size_unit: SizeUnit,
    tif: TimeInForce,
    #[serde(default)]
    paper: bool,
}

/// Fully validated intent ready for origin-stamping.
struct ValidatedIntent {
    market: MarketKey,
    side: Side,
    order_type: OrderType,
    limit_price: Option<Decimal>,
    size: Decimal,
    size_unit: SizeUnit,
    tif: TimeInForce,
    paper: bool,
}

impl ClientIntentBody {
    /// Validate decimal fields. Enums and MarketKey are already validated
    /// by serde during deserialization.
    fn validate(self) -> Result<ValidatedIntent, String> {
        let size = Decimal::from_str_exact(&self.size)
            .map_err(|e| format!("invalid size '{}': {e}", self.size))?;
        let limit_price = self
            .limit_price
            .filter(|s| !s.is_empty())
            .map(|s| {
                Decimal::from_str_exact(&s).map_err(|e| format!("invalid limit_price '{s}': {e}"))
            })
            .transpose()?;
        Ok(ValidatedIntent {
            market: self.market,
            side: self.side,
            order_type: self.order_type,
            limit_price,
            size,
            size_unit: self.size_unit,
            tif: self.tif,
            paper: self.paper,
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
fn session_origin_kind(kind: &str) -> OriginKind {
    match kind {
        "user" => OriginKind::User,
        "alert_action" => OriginKind::AlertAction,
        "agent" => OriginKind::Agent,
        "automation" => OriginKind::Automation,
        _ => OriginKind::User, // default: treat unknown as user-origin
    }
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
        ClientFrame::Command { id, trace_id, text, .. } => {
            let trace_id = make_trace_id(&id, &trace_id);
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "echo": text,
                    "note": "MCP stub — command echo only",
                    "actor_id": session.actor_id,
                    "origin_kind": session.origin.kind,
                }),
            }
        }
        ClientFrame::OrderIntent { id, trace_id, body } => {
            let trace_id = make_trace_id(&id, &trace_id);

            // Phase 1: deserialize into domain types (rejects invalid enums,
            // malformed market keys, missing mandatory fields).
            let parsed: ClientIntentBody = match serde_json::from_value(body) {
                Ok(p) => p,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::InvalidArgument,
                            format!("invalid order_intent: {e}"),
                            aether_core::Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 2: validate decimal strings (size, limit_price).
            let validated = match parsed.validate() {
                Ok(v) => v,
                Err(e) => {
                    return ServerFrame::Error {
                        id,
                        trace_id,
                        body: aether_core::ErrorEnvelope::new(
                            aether_core::ErrorCode::InvalidArgument,
                            e,
                            aether_core::Ulid::new(),
                        ),
                    };
                }
            };

            // Phase 3: stamp the canonical Origin from the authenticated session.
            // Origin is NEVER taken from the client body.
            let origin_kind = session_origin_kind(&session.origin.kind);
            let _origin = Origin::new(
                origin_kind,
                session.tier as u8,
                aether_core::Ulid::new(), // stub: generate new id; real impl parses session.actor_id
            );

            // Build a human-readable summary from the validated domain types.
            let action_summary = format!(
                "{order_type:?} {side:?} {size} {size_unit:?} {market} (paper={paper})",
                order_type = validated.order_type,
                side = validated.side,
                size = validated.size,
                size_unit = validated.size_unit,
                market = validated.market,
                paper = validated.paper,
            );

            ServerFrame::ConfirmRequired {
                id,
                trace_id,
                ref_id: uuid::Uuid::new_v4().to_string(),
                action_summary,
                tier_reason: format!(
                    "EP-401: tier {} not enforced yet — allow-with-note",
                    session.tier
                ),
                // Origin stamped from the authenticated session, NOT from client body
                actor_id: session.actor_id.clone(),
                origin_kind: session.origin.kind.clone(),
            }
        }
        ClientFrame::Confirm { id, trace_id, ref_id, .. } => {
            let trace_id = make_trace_id(&id, &trace_id);
            ServerFrame::CommandResult {
                id,
                trace_id,
                body: serde_json::json!({
                    "status": "confirmed",
                    "ref_id": ref_id,
                    "note": "stub — order not actually executed",
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

    /// A valid minimal OrderIntent body payload — uses proper domain types.
    const VALID_INTENT_BODY: &str = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;

    fn test_session() -> auth::SessionInfo {
        auth::SessionInfo {
            actor_id: "alice".into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: "alice".into() },
        }
    }

    fn make_intent(body: &str) -> String {
        format!(r#"{{"type":"order_intent","body":{body}}}"#)
    }

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
    }

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
            actor_id: "bob".into(),
            tier: 1,
            origin: auth::OriginInfo { kind: "automation".into(), actor_id: "bob".into() },
        };
        let oi = make_intent(VALID_INTENT_BODY);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"actor_id\":\"bob\""), "should contain actor_id: {json}");
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
        // The action_summary uses Debug formatting of domain enums (PascalCase)
        assert!(json.contains("mkt:kalshi:BTC-75"), "missing market: {json}");
        assert!(json.contains("Sell"), "missing side: {json}");
        assert!(json.contains("Market"), "missing order_type: {json}");
        assert!(json.contains("1.5"), "missing size: {json}");
        assert!(json.contains("Base"), "missing size_unit: {json}");
        // Origin is from the session, not the client
        assert!(json.contains("\"actor_id\":\"alice\""), "origin should be session actor");
        assert!(json.contains("\"origin_kind\":\"user\""), "origin_kind should be session origin");
    }

    #[test]
    fn order_intent_invalid_body_type_returns_error() {
        // A JSON number cannot be deserialized into ClientIntentBody (struct expected).
        let oi = r#"{"type":"order_intent","body":42}"#;
        let frame: ClientFrame = serde_json::from_str(oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""), "invalid body type should produce error: {json}");
        assert!(json.contains("invalid_argument"), "should use invalid_argument: {json}");
    }

    #[test]
    fn order_intent_origin_never_from_client() {
        // Client sends extra fields in body — origin MUST still come from session.
        let session = auth::SessionInfo {
            actor_id: "trusted-system".into(),
            tier: 4,
            origin: auth::OriginInfo { kind: "agent".into(), actor_id: "trusted-system".into() },
        };
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"sell","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","extra_field":"ignored"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"origin_kind\":\"agent\""), "origin_kind from session: {json}");
        assert!(json.contains("\"actor_id\":\"trusted-system\""), "actor_id from session: {json}");
        assert!(!json.contains("extra_field"), "unknown fields must not leak: {json}");
    }

    // ── Adversarial tests ────────────────────────────────────────────

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
            actor_id: "real-user".into(),
            tier: 2,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: "real-user".into() },
        };
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","origin_kind":"attacker","actor_id":"evil"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &session);
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("attacker"), "client origin_kind must not leak: {json}");
        assert!(!json.contains("evil"), "client actor_id must not leak: {json}");
        assert!(json.contains("\"origin_kind\":\"user\""), "origin from session: {json}");
        assert!(json.contains("\"actor_id\":\"real-user\""), "actor_id from session: {json}");
    }

    #[test]
    fn intent_limit_price_valid_decimal_accepted() {
        let body = r#"{"market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","limit_price":"65000.50","size":"0.01","size_unit":"contracts","tif":"gtc"}"#;
        let oi = make_intent(body);
        let frame: ClientFrame = serde_json::from_str(&oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"), "valid limit_price must be accepted: {json}");
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
}
