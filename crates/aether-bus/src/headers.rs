//! Kafka message header constants and helpers for AETHER envelope propagation.
//!
//! Every produced message carries three headers:
//! - `trace_id` — the envelope's trace identifier for distributed tracing
//! - `schema` — the schema identifier (e.g. `aether.Quote.v1`)
//! - `content-type` — always `application/json`

use rdkafka::message::Header;
use rdkafka::message::Headers;
use rdkafka::message::OwnedHeaders;
use rdkafka::producer::FutureRecord;

/// Header key for the trace ID across Kafka messages.
pub const KAFKA_HEADER_TRACE_ID: &str = "trace_id";

/// Header key for the schema identifier of the envelope payload.
pub const KAFKA_HEADER_SCHEMA: &str = "schema";

/// Header key for the content type of the payload.
pub const KAFKA_HEADER_CONTENT_TYPE: &str = "content-type";

/// Add the standard AETHER tracing headers to a [`FutureRecord`].
///
/// Sets `trace_id`, `schema`, and `content-type` headers.
pub fn add_headers<'a>(
    record: FutureRecord<'a, [u8], [u8]>,
    trace_id: &str,
    schema: &str,
) -> FutureRecord<'a, [u8], [u8]> {
    let headers = OwnedHeaders::new()
        .insert(Header { key: KAFKA_HEADER_TRACE_ID, value: Some(trace_id.as_bytes()) })
        .insert(Header { key: KAFKA_HEADER_SCHEMA, value: Some(schema.as_bytes()) })
        .insert(Header {
            key: KAFKA_HEADER_CONTENT_TYPE,
            value: Some(b"application/json" as &[u8]),
        });
    record.headers(headers)
}

/// Extract the trace_id from a consumed message's Kafka headers.
pub fn extract_trace_id(msg: &rdkafka::message::BorrowedMessage<'_>) -> Option<String> {
    use rdkafka::message::Message;
    msg.headers().and_then(|headers| {
        headers
            .iter()
            .find(|h| h.key == KAFKA_HEADER_TRACE_ID)
            .and_then(|h| h.value.map(|v| String::from_utf8(v.to_vec()).ok()))
            .flatten()
    })
}

/// Extract the schema from a consumed message's Kafka headers.
pub fn extract_schema(msg: &rdkafka::message::BorrowedMessage<'_>) -> Option<String> {
    use rdkafka::message::Message;
    msg.headers().and_then(|headers| {
        headers
            .iter()
            .find(|h| h.key == KAFKA_HEADER_SCHEMA)
            .and_then(|h| h.value.map(|v| String::from_utf8(v.to_vec()).ok()))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_setting_produces_roundtrippable_headers() {
        let headers = OwnedHeaders::new()
            .insert(Header { key: KAFKA_HEADER_TRACE_ID, value: Some(b"t1" as &[u8]) })
            .insert(Header { key: KAFKA_HEADER_SCHEMA, value: Some(b"s1" as &[u8]) })
            .insert(Header {
                key: KAFKA_HEADER_CONTENT_TYPE,
                value: Some(b"application/json" as &[u8]),
            });

        assert_eq!(
            headers
                .iter()
                .find(|h| h.key == KAFKA_HEADER_TRACE_ID)
                .and_then(|h| h.value)
                .map(|v| std::str::from_utf8(v).unwrap()),
            Some("t1")
        );
        assert_eq!(
            headers
                .iter()
                .find(|h| h.key == KAFKA_HEADER_SCHEMA)
                .and_then(|h| h.value)
                .map(|v| std::str::from_utf8(v).unwrap()),
            Some("s1")
        );
        assert_eq!(
            headers
                .iter()
                .find(|h| h.key == KAFKA_HEADER_CONTENT_TYPE)
                .and_then(|h| h.value)
                .map(|v| std::str::from_utf8(v).unwrap()),
            Some("application/json")
        );
    }

    #[test]
    fn extract_from_empty_headers_returns_none() {
        let headers = OwnedHeaders::new();
        assert!(headers.iter().find(|h| h.key == KAFKA_HEADER_TRACE_ID).is_none());
        assert!(headers.iter().find(|h| h.key == KAFKA_HEADER_SCHEMA).is_none());
    }
}
