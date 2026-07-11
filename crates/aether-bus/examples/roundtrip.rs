use aether_bus::consumer::{MessageConsumer, StubConsumer};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, StubProducer};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DemoQuote {
    market: String,
    bid: String,
    ask: String,
}

fn main() {
    let shared: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let producer = StubProducer { sent: shared.clone() };
    let consumer = StubConsumer::new(shared);

    let quote =
        DemoQuote { market: "mkt:kalshi:BTC-75".into(), bid: "0.65".into(), ask: "0.67".into() };
    let envelope = Envelope::new("Quote", quote);
    producer.send("md.ticks.kalshi", envelope).unwrap();

    let received: Vec<Envelope<DemoQuote>> = consumer.consume().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].payload,
        DemoQuote { market: "mkt:kalshi:BTC-75".into(), bid: "0.65".into(), ask: "0.67".into() }
    );
    println!("Round-trip OK: envelope produced and consumed successfully");
}
