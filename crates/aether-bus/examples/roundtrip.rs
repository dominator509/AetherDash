use aether_bus::consumer::{KafkaConsumer, MessageConsumer, StubConsumer};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{KafkaProducer, MessageProducer, StubProducer};
use aether_bus::StubObjectStore;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct DemoQuote {
    market: String,
    bid: String,
    ask: String,
}

fn quote() -> DemoQuote {
    DemoQuote { market: "mkt:kalshi:BTC-75".into(), bid: "0.65".into(), ask: "0.67".into() }
}

#[tokio::main]
async fn main() {
    let bootstrap = std::env::var("AETHER_KAFKA_BOOTSTRAP").ok();

    if let Some(ref servers) = bootstrap {
        kafka_roundtrip(servers).await;
    } else {
        stub_roundtrip().await;
    }
}

/// Round-trip using real KafkaProducer / KafkaConsumer.
async fn kafka_roundtrip(servers: &str) {
    let producer = KafkaProducer::new(servers).unwrap_or_else(|e| panic!("producer: {e}"));

    let topic = "md.ticks.demo.example";
    let group = "svc.aether-bus-example";
    let dq = quote();

    // Send without partition key (round-robin)
    let envelope = Envelope::new("Quote", dq.clone());
    producer.send(topic, envelope, None).await.unwrap_or_else(|e| panic!("send: {e}"));

    // Send with partition key (consistent hash)
    let envelope2 = Envelope::new("Quote", dq.clone());
    producer
        .send(topic, envelope2, Some("mkt:kalshi:BTC-75"))
        .await
        .unwrap_or_else(|e| panic!("send with key: {e}"));

    let consumer = KafkaConsumer::new(
        servers,
        group,
        producer,
        Arc::new(StubObjectStore::new()) as Arc<dyn aether_bus::ObjectStore>,
    )
    .unwrap_or_else(|e| panic!("consumer: {e}"));

    let received: Vec<Envelope<DemoQuote>> =
        consumer.consume(&[topic]).await.unwrap_or_else(|e| panic!("consume: {e}"));

    assert!(!received.is_empty(), "expected at least one envelope");
    assert_eq!(received[0].payload, dq, "payload mismatch");
    println!(
        "Kafka round-trip OK: {} envelope(s) produced and consumed successfully",
        received.len()
    );
}

/// Round-trip using in-memory StubProducer / StubConsumer.
async fn stub_roundtrip() {
    let shared: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let producer = StubProducer { sent: shared.clone() };
    let consumer = StubConsumer::new(shared);

    let envelope = Envelope::new("Quote", quote());
    producer
        .send("md.ticks.kalshi", envelope, None)
        .await
        .unwrap_or_else(|e| panic!("send failed: {e}"));

    let received: Vec<Envelope<DemoQuote>> = consumer
        .consume(&["md.ticks.kalshi"])
        .await
        .unwrap_or_else(|e| panic!("consume failed: {e}"));

    assert_eq!(received.len(), 1);
    assert_eq!(received[0].payload, quote());
    println!("Stub round-trip OK: envelope produced and consumed successfully");
}
