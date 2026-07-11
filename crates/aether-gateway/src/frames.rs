use crate::auth;
use serde::{Deserialize, Serialize};

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
        ClientFrame::OrderIntent { id, trace_id, .. } => {
            let trace_id = make_trace_id(&id, &trace_id);
            ServerFrame::ConfirmRequired {
                id,
                trace_id,
                ref_id: uuid::Uuid::new_v4().to_string(),
                action_summary: "paper order intent received (stub)".into(),
                tier_reason: "EP-401: tier not enforced yet — allow-with-note".into(),
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

    fn test_session() -> auth::SessionInfo {
        auth::SessionInfo {
            actor_id: "alice".into(),
            tier: 3,
            origin: auth::OriginInfo { kind: "user".into(), actor_id: "alice".into() },
        }
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
        let cases = [
            (r#"{"type":"subscribe","channels":[]}"#, true),
            (r#"{"type":"unsubscribe"}"#, true),
            (r#"{"type":"command","text":"help"}"#, true),
            (r#"{"type":"order_intent","body":{}}"#, true),
            (r#"{"type":"confirm","ref_id":"abc","totp":null}"#, true),
            (r#"{"type":"ping"}"#, true),
        ];
        for (json, expect_ok) in &cases {
            let result: Result<ClientFrame, _> = serde_json::from_str(json);
            assert_eq!(result.is_ok(), *expect_ok, "failed for: {json}");
        }
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
        let oi = r#"{"type":"order_intent","body":{"market":"BTC-USD","side":"buy"}}"#;
        let frame: ClientFrame = serde_json::from_str(oi).unwrap();
        let response = dispatch(frame, &test_session());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("confirm_required"));
        assert!(json.contains("ref_id"));
        assert!(json.contains("actor_id"));
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
        let cases: Vec<(&str, &str)> = vec![
            (r#"{"type":"subscribe","channels":["a"]}"#, "command_result"),
            (r#"{"type":"unsubscribe","id":"u1"}"#, "command_result"),
            (r#"{"type":"command","text":"hi"}"#, "command_result"),
            (r#"{"type":"order_intent","body":{}}"#, "confirm_required"),
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
        let oi = r#"{"type":"order_intent","body":{"market":"ETH-USD"}}"#;
        let frame: ClientFrame = serde_json::from_str(oi).unwrap();
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
}
