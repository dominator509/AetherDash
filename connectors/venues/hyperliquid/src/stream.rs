//! Hyperliquid polling stream client.
//!
//! Since Hyperliquid does not offer a public WebSocket for market data,
//! this module provides polling-based streams that periodically fetch
//! `allMids` and `l2Book` and emit normalized `Quote` / `OrderBook`
//! events to the message bus.
//!
//! # Reconnection & backoff
//!
//! Uses SPEC-006 full-jitter exponential backoff on HTTP errors. A simple
//! circuit breaker opens after 5 consecutive errors and prevents further
//! polling until reset.

use crate::client::HlClient;
use crate::normalize::{normalize_book, normalize_mid_to_quote};
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use aether_bus::producer::ProducerError;
use aether_bus::quarantine::{ObjectStore, Quarantine, QuarantineError};
use aether_bus::retry::{CircuitBreaker, RetryPolicy};
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum consecutive polling errors before the circuit breaker opens.
const BREAKER_THRESHOLD: u32 = 5;

/// Base delay for exponential backoff (milliseconds).
const BACKOFF_BASE_MS: u64 = 200;

/// Maximum delay cap for exponential backoff (milliseconds).
const BACKOFF_CAP_MS: u64 = 30_000;

/// Polling interval for allMids in milliseconds.
const MIDS_POLL_MS: u64 = 2_000;

/// Polling interval for l2Book per coin in milliseconds.
const BOOK_POLL_MS: u64 = 2_000;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Hyperliquid polling stream.
#[derive(Error, Debug)]
pub enum StreamError {
    /// Client error during API call.
    #[error("client error: {0}")]
    Client(#[from] crate::client::ClientError),

    /// Normalization pipeline error.
    #[error("normalization error: {0}")]
    Normalize(#[from] crate::normalize::NormalizeError),

    /// Message producer error.
    #[error("producer error: {0}")]
    Producer(#[from] ProducerError),

    /// Malformed payload could not be preserved and published to quarantine.
    #[error("quarantine error: {0}")]
    Quarantine(#[from] QuarantineError),

    /// Circuit breaker tripped — too many consecutive failures.
    #[error("circuit breaker open after {threshold} consecutive failures")]
    BreakerOpen {
        /// The breaker threshold that was exceeded.
        threshold: u32,
    },
}

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

/// A simple failure-count circuit breaker for polling.
#[derive(Debug)]
pub struct StreamBreaker {
    threshold: u32,
    inner: CircuitBreaker,
}

impl StreamBreaker {
    pub fn new(threshold: u32) -> Self {
        debug_assert_eq!(threshold, CircuitBreaker::SPEC_DEFAULT_THRESHOLD);
        Self { threshold, inner: CircuitBreaker::new() }
    }

    pub fn record_failure(&mut self) -> Result<(), StreamError> {
        self.inner.record_failure();
        if !self.inner.allow_request() {
            return Err(StreamError::BreakerOpen { threshold: self.threshold });
        }
        Ok(())
    }

    pub fn record_success(&mut self) {
        self.inner.record_success();
    }

    pub fn is_open(&mut self) -> bool {
        !self.inner.allow_request()
    }
}

// ---------------------------------------------------------------------------
// SPEC-006 full-jitter backoff
// ---------------------------------------------------------------------------

fn full_jitter_backoff(attempt: u32, base_ms: u64, cap_ms: u64) -> Duration {
    RetryPolicy {
        max_attempts: 5,
        base_delay: Duration::from_millis(base_ms),
        max_delay: Duration::from_millis(cap_ms),
    }
    .delay_for_attempt(attempt)
}

// ---------------------------------------------------------------------------
// HlStream
// ---------------------------------------------------------------------------

/// A polling-based stream client for Hyperliquid market data.
#[derive(Debug)]
pub struct HlStream {
    /// The underlying Hyperliquid REST client.
    pub client: HlClient,
}

impl HlStream {
    /// Create a new stream client wrapping an `HlClient`.
    pub fn new(client: HlClient) -> Self {
        Self { client }
    }

    /// Create a new stream client from environment variables.
    pub fn from_env() -> Self {
        Self::new(HlClient::from_env())
    }

    // -----------------------------------------------------------------------
    // Mid price polling
    // -----------------------------------------------------------------------

    /// Poll `allMids` at a fixed interval and emit normalized `Quote`s to
    /// `md.ticks.hyperliquid`.
    ///
    /// This method runs an infinite loop:
    /// 1. Fetch `allMids`
    /// 2. For each requested ticker that appears in the response, normalize
    ///    to a `Quote` and publish
    /// 3. Sleep for `MIDS_POLL_MS`
    /// 4. On error, back off and retry
    /// 5. On >5 consecutive errors, open the circuit breaker and return
    pub async fn stream_mids_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.ticks.hyperliquid";
        let mut attempt: u32 = 0;
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        let interval = Duration::from_millis(MIDS_POLL_MS);

        loop {
            if breaker.is_open() {
                return Err(StreamError::BreakerOpen { threshold: BREAKER_THRESHOLD });
            }

            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "retrying mids poll");
                tokio::time::sleep(delay).await;
            }

            let now_ms = chrono::Utc::now().timestamp_millis();

            match self.client.get_all_mids().await {
                Ok(mids) => {
                    attempt = 0;
                    breaker.record_success();

                    for ticker in tickers {
                        let upper = ticker.to_uppercase();
                        let response_key = if upper == "@0" { "PURR/USDC" } else { &upper };
                        let mid_str = match mids.get(response_key) {
                            Some(value) => value.as_str().unwrap_or("").to_string(),
                            None => continue,
                        };

                        if mid_str.is_empty() {
                            continue;
                        }

                        match normalize_mid_to_quote(ticker, &mid_str, now_ms) {
                            Ok(quote) => {
                                let envelope = Envelope::new("quote", &quote);
                                if let Err(e) = producer
                                    .send(topic, envelope, Some(quote.market.as_str()))
                                    .await
                                {
                                    return Err(StreamError::Producer(e));
                                }
                            }
                            Err(e) => {
                                quarantine_payload(
                                    quarantine_producer,
                                    quarantine_storage,
                                    "hyperliquid",
                                    &e.to_string(),
                                    mid_str.as_bytes(),
                                )
                                .await?;
                                warn!(error = %e, ticker, "mid normalization failed");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "allMids poll failed");
                    attempt += 1;
                    if let Err(be) = breaker.record_failure() {
                        error!("circuit breaker opened: {be}");
                        return Err(be);
                    }
                    continue;
                }
            }

            tokio::time::sleep(interval).await;
        }
    }

    // -----------------------------------------------------------------------
    // Order book polling
    // -----------------------------------------------------------------------

    /// Poll `l2Book` for each ticker at a fixed interval and emit normalized
    /// `OrderBook`s to `md.books.hyperliquid`.
    ///
    /// This method runs an infinite loop:
    /// 1. For each ticker, fetch `l2Book`
    /// 2. Normalize to `OrderBook` and publish
    /// 3. Sleep for `BOOK_POLL_MS`
    /// 4. On error, back off and retry
    /// 5. On >5 consecutive errors, open the circuit breaker and return
    pub async fn stream_books_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.books.hyperliquid";
        let mut attempt: u32 = 0;
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        let interval = Duration::from_millis(BOOK_POLL_MS);

        loop {
            if breaker.is_open() {
                return Err(StreamError::BreakerOpen { threshold: BREAKER_THRESHOLD });
            }

            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "retrying book poll");
                tokio::time::sleep(delay).await;
            }

            let mut any_ok = false;

            for ticker in tickers {
                match self.client.get_l2_book(ticker).await {
                    Ok(snapshot) => {
                        any_ok = true;
                        match normalize_book(&snapshot) {
                            Ok(book) => {
                                let envelope = Envelope::new("order_book", &book);
                                if let Err(e) =
                                    producer.send(topic, envelope, Some(book.market.as_str())).await
                                {
                                    return Err(StreamError::Producer(e));
                                }
                            }
                            Err(e) => {
                                let raw = serde_json::to_vec(&snapshot).unwrap_or_default();
                                quarantine_payload(
                                    quarantine_producer,
                                    quarantine_storage,
                                    "hyperliquid",
                                    &e.to_string(),
                                    &raw,
                                )
                                .await?;
                                warn!(error = %e, ticker, "book normalization failed");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, ticker, "l2Book poll failed");
                    }
                }
            }

            if any_ok {
                attempt = 0;
                breaker.record_success();
            } else {
                attempt += 1;
                if let Err(be) = breaker.record_failure() {
                    error!("circuit breaker opened: {be}");
                    return Err(be);
                }
                continue;
            }

            tokio::time::sleep(interval).await;
        }
    }
}

/// Publish a raw payload to the quarantine topic.
async fn quarantine_payload<Q: MessageProducer, S: ObjectStore>(
    producer: &Q,
    storage: &S,
    venue: &str,
    reason: &str,
    raw_payload: &[u8],
) -> Result<String, QuarantineError> {
    Quarantine::publish(producer, storage, venue, reason, raw_payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;
    use aether_bus::quarantine::StubObjectStore;

    #[tokio::test]
    async fn malformed_mid_is_preserved_and_published_to_venue_quarantine() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();
        let raw = b"not-a-real-number";

        let hash =
            quarantine_payload(&producer, &storage, "hyperliquid", "bad price", raw).await.unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "quarantine.hyperliquid");
        drop(sent);
        let stored = storage.objects.lock().unwrap();
        assert_eq!(stored.get(&format!("quarantine/hyperliquid/{hash}")), Some(&raw.to_vec()));
    }

    // ── Backoff ──

    #[test]
    fn full_jitter_backoff_bounds() {
        let d = full_jitter_backoff(0, 200, 30_000);
        assert!(d.as_millis() <= 200);

        let d = full_jitter_backoff(20, 200, 30_000);
        assert!(d.as_millis() <= 30_000);
    }

    // ── Breaker ──

    #[test]
    fn breaker_opens_after_threshold() {
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        assert!(!breaker.is_open());

        for _ in 0..(BREAKER_THRESHOLD - 1) {
            assert!(breaker.record_failure().is_ok());
            assert!(!breaker.is_open());
        }
        let result = breaker.record_failure();
        assert!(result.is_err());
        assert!(matches!(result, Err(StreamError::BreakerOpen { threshold: 5 })));
        assert!(breaker.is_open());
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        breaker.record_failure().unwrap();
        breaker.record_failure().unwrap();
        breaker.record_success();
        let result = breaker.record_failure();
        assert!(result.is_ok());
    }
}
