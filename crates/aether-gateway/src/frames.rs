use serde::{Deserialize, Serialize};

// ── Client → Server frames ──
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientFrame {
    #[serde(rename = "subscribe")]
    Subscribe { id: Option<String>, channels: Vec<String> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { id: Option<String> },
    #[serde(rename = "command")]
    Command { id: Option<String>, text: String, room_context: Option<String> },
    #[serde(rename = "order_intent")]
    OrderIntent { id: Option<String>, body: serde_json::Value },
    #[serde(rename = "confirm")]
    Confirm { id: Option<String>, ref_id: String, totp: Option<String> },
    #[serde(rename = "ping")]
    Ping { id: Option<String> },
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
    ConfirmRequired { id: Option<String>, ref_id: String, action_summary: String, tier_reason: String },
    #[serde(rename = "degradation")]
    Degradation { id: Option<String>, surface: String, reason: String },
    #[serde(rename = "error")]
    Error { id: Option<String>, trace_id: Option<String>, body: aether_core::error::ErrorEnvelope },
    #[serde(rename = "pong")]
    Pong { id: Option<String> },
}

/// Dispatch a client frame to its server-frame response.
/// Stub: all frames echo back as pong (real dispatch in EP-201/EP-305).
pub fn dispatch(frame: ClientFrame) -> ServerFrame {
    match frame {
        ClientFrame::Subscribe { id, .. } => ServerFrame::CommandResult {
            id,
            trace_id: None,
            body: serde_json::json!({"status": "subscribed", "note": "stub — all channels accepted"}),
        },
        ClientFrame::Ping { id } => ServerFrame::Pong { id },
        ClientFrame::Command { id, text, .. } => ServerFrame::CommandResult {
            id,
            trace_id: None,
            body: serde_json::json!({"echo": text, "note": "MCP stub — command echo only"}),
        },
        ClientFrame::OrderIntent { id, .. } => ServerFrame::ConfirmRequired {
            id,
            ref_id: uuid::Uuid::new_v4().to_string(),
            action_summary: "paper order intent received (stub)".into(),
            tier_reason: "EP-401: tier not enforced yet — allow-with-note".into(),
        },
        _ => ServerFrame::Pong { id: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_pong_round_trip() {
        let ping = r#"{"type":"ping","id":"1"}"#;
        let frame: ClientFrame = serde_json::from_str(ping).unwrap();
        let response = dispatch(frame);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"pong\""));
    }

    #[test]
    fn subscribe_returns_command_result() {
        let sub = r#"{"type":"subscribe","channels":["quotes:mkt:kalshi:BTC-75"]}"#;
        let frame: ClientFrame = serde_json::from_str(sub).unwrap();
        let response = dispatch(frame);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("subscribed"));
    }

    #[test]
    fn unknown_type_is_error() {
        let unknown = r#"{"type":"bad_frame","x":1}"#;
        let result: Result<ClientFrame, _> = serde_json::from_str(unknown);
        assert!(result.is_err(), "unknown frame type must fail deserialization");
    }

    #[test]
    fn all_frame_types_round_trip() {
        // Test every known frame type deserializes correctly
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
}
