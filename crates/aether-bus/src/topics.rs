/// Topic constants — the single source of truth for bus topic names.
/// SPEC-003: "topics live here or they do not exist."
pub struct Topic;
impl Topic {
    pub const MD_TICKS: &str = "md.ticks";
    pub const MD_BOOKS: &str = "md.books";
    pub const QUARANTINE: &str = "quarantine";
    pub const BRAIN_OBJECTS: &str = "brain.objects";
    pub const OPPS_DETECTED: &str = "opps.detected";
    pub const ORDERS_INTENTS: &str = "orders.intents";
    pub const ORDERS_FILLS: &str = "orders.fills";
    pub const ALERTS_OUTBOUND: &str = "alerts.outbound";
    pub const AUDIT_EVENTS: &str = "audit.events";
}

/// Build a venue-specific topic string. E.g. `topic_for(Topic::MD_TICKS, "kalshi")` -> "md.ticks.kalshi"
pub fn topic_for(base: &str, venue: &str) -> String {
    format!("{base}.{venue}")
}

/// Standard consumer group names per SPEC-003.
pub struct ConsumerGroup;
impl ConsumerGroup {
    pub fn for_service(name: &str) -> String {
        format!("svc.{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topic_for_md_ticks() {
        assert_eq!(topic_for(Topic::MD_TICKS, "kalshi"), "md.ticks.kalshi");
    }
    #[test]
    fn consumer_group_naming() {
        assert_eq!(ConsumerGroup::for_service("gateway"), "svc.gateway");
    }
    #[test]
    fn topics_are_unique() {
        let topics = [
            Topic::MD_TICKS,
            Topic::MD_BOOKS,
            Topic::QUARANTINE,
            Topic::BRAIN_OBJECTS,
            Topic::OPPS_DETECTED,
            Topic::ORDERS_INTENTS,
            Topic::ORDERS_FILLS,
            Topic::ALERTS_OUTBOUND,
            Topic::AUDIT_EVENTS,
        ];
        let mut seen = std::collections::HashSet::new();
        for t in &topics {
            assert!(seen.insert(t), "Duplicate topic: {t}");
        }
    }
}
