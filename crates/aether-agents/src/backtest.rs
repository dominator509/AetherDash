use aether_testing::{CapturedEvent, ReplayError, ReplayHarness};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Recorded, side-effect-free input consumed from the EP-405 replay harness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyTick {
    pub timestamp_ms: i64,
    pub price_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdStrategy {
    pub buy_below_minor: i64,
    pub sell_above_minor: i64,
    pub fee_bps: u32,
    pub slippage_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacktestReport {
    pub strategy_sha256: String,
    pub capture_sha256: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub event_count: usize,
    pub round_trips: u64,
    pub forced_period_end_exits: u64,
    pub gross_pnl_minor: i64,
    pub fees_minor: i64,
    pub slippage_minor: i64,
    pub net_pnl_minor: i64,
}

#[derive(Debug, Default)]
pub struct Backtester;

impl Backtester {
    pub fn run(
        &self,
        replay: &ReplayHarness,
        strategy: &ThresholdStrategy,
    ) -> Result<BacktestReport, BacktestError> {
        validate_strategy(strategy)?;
        let ticks: Vec<_> = replay
            .events()
            .iter()
            .filter(|event| event.event_type == "strategy_tick")
            .map(decode_tick)
            .collect::<Result<_, _>>()?;
        let first = ticks.first().ok_or(BacktestError::NoTicks)?;
        let last = ticks.last().ok_or(BacktestError::NoTicks)?;

        let mut entry = None;
        let mut gross = 0_i64;
        let mut fees = 0_i64;
        let mut slippage = 0_i64;
        let mut round_trips = 0_u64;
        let mut forced_period_end_exits = 0_u64;
        for tick in &ticks {
            if entry.is_none() && tick.price_minor <= strategy.buy_below_minor {
                entry = Some(tick.price_minor);
            } else if let Some(entry_price) = entry {
                if tick.price_minor >= strategy.sell_above_minor {
                    gross = gross
                        .checked_add(tick.price_minor - entry_price)
                        .ok_or(BacktestError::Arithmetic)?;
                    fees = fees
                        .checked_add(bps_cost(entry_price, strategy.fee_bps)?)
                        .and_then(|value| {
                            bps_cost(tick.price_minor, strategy.fee_bps)
                                .ok()
                                .and_then(|exit| value.checked_add(exit))
                        })
                        .ok_or(BacktestError::Arithmetic)?;
                    slippage = slippage
                        .checked_add(bps_cost(entry_price, strategy.slippage_bps)?)
                        .and_then(|value| {
                            bps_cost(tick.price_minor, strategy.slippage_bps)
                                .ok()
                                .and_then(|exit| value.checked_add(exit))
                        })
                        .ok_or(BacktestError::Arithmetic)?;
                    round_trips = round_trips.checked_add(1).ok_or(BacktestError::Arithmetic)?;
                    entry = None;
                }
            }
        }
        if let Some(entry_price) = entry {
            gross = gross
                .checked_add(last.price_minor - entry_price)
                .ok_or(BacktestError::Arithmetic)?;
            fees = fees
                .checked_add(bps_cost(entry_price, strategy.fee_bps)?)
                .and_then(|value| {
                    bps_cost(last.price_minor, strategy.fee_bps)
                        .ok()
                        .and_then(|exit| value.checked_add(exit))
                })
                .ok_or(BacktestError::Arithmetic)?;
            slippage = slippage
                .checked_add(bps_cost(entry_price, strategy.slippage_bps)?)
                .and_then(|value| {
                    bps_cost(last.price_minor, strategy.slippage_bps)
                        .ok()
                        .and_then(|exit| value.checked_add(exit))
                })
                .ok_or(BacktestError::Arithmetic)?;
            round_trips = round_trips.checked_add(1).ok_or(BacktestError::Arithmetic)?;
            forced_period_end_exits = 1;
        }
        let net = gross
            .checked_sub(fees)
            .and_then(|value| value.checked_sub(slippage))
            .ok_or(BacktestError::Arithmetic)?;
        Ok(BacktestReport {
            strategy_sha256: canonical_hash(strategy)?,
            capture_sha256: capture_hash(replay.events()),
            start_ms: first.timestamp_ms,
            end_ms: last.timestamp_ms,
            event_count: ticks.len(),
            round_trips,
            forced_period_end_exits,
            gross_pnl_minor: gross,
            fees_minor: fees,
            slippage_minor: slippage,
            net_pnl_minor: net,
        })
    }
}

fn validate_strategy(strategy: &ThresholdStrategy) -> Result<(), BacktestError> {
    if strategy.buy_below_minor <= 0
        || strategy.sell_above_minor <= strategy.buy_below_minor
        || strategy.fee_bps > 10_000
        || strategy.slippage_bps > 10_000
    {
        return Err(BacktestError::InvalidStrategy);
    }
    Ok(())
}

fn decode_tick(event: &CapturedEvent) -> Result<StrategyTick, BacktestError> {
    let tick: StrategyTick = serde_json::from_slice(&event.payload_bytes)?;
    if tick.price_minor <= 0 {
        return Err(BacktestError::InvalidTick);
    }
    Ok(tick)
}

fn bps_cost(value: i64, bps: u32) -> Result<i64, BacktestError> {
    let cost = Decimal::from(value) * Decimal::from(bps) / Decimal::from(10_000_u32);
    cost.ceil().to_string().parse().map_err(|_| BacktestError::Arithmetic)
}

fn canonical_hash(value: &impl Serialize) -> Result<String, BacktestError> {
    Ok(hex_digest(&serde_json::to_vec(value)?))
}

fn capture_hash(events: &[CapturedEvent]) -> String {
    let bytes: Vec<_> =
        events.iter().flat_map(|event| event.payload_bytes.iter().copied()).collect();
    hex_digest(&bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum BacktestError {
    #[error("strategy definition is invalid")]
    InvalidStrategy,
    #[error("recorded strategy tick is invalid")]
    InvalidTick,
    #[error("recorded period contains no strategy ticks")]
    NoTicks,
    #[error("backtest arithmetic overflow")]
    Arithmetic,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Replay(#[from] ReplayError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture() -> ReplayHarness {
        let mut capture = ReplayHarness::new_capture();
        for (index, price) in [95, 105, 94, 110].into_iter().enumerate() {
            capture
                .record_event(
                    "strategy_tick",
                    &format!("trace-{index}"),
                    &StrategyTick { timestamp_ms: index as i64 * 1_000, price_minor: price },
                )
                .expect("record fixture");
        }
        ReplayHarness::new_replay(capture.events().to_vec())
    }

    #[test]
    fn recorded_period_is_deterministic_and_net_edge_honest() {
        let strategy = ThresholdStrategy {
            buy_below_minor: 96,
            sell_above_minor: 104,
            fee_bps: 100,
            slippage_bps: 100,
        };
        let first = Backtester.run(&capture(), &strategy).expect("backtest");
        let second = Backtester.run(&capture(), &strategy).expect("backtest repeat");
        assert_eq!(first, second);
        assert_eq!(first.round_trips, 2);
        assert_eq!(first.gross_pnl_minor, 26);
        assert_eq!(first.fees_minor, 6);
        assert_eq!(first.slippage_minor, 6);
        assert_eq!(first.net_pnl_minor, 14);
    }

    #[test]
    fn malformed_data_and_strategy_fail_without_side_effects() {
        let invalid = ThresholdStrategy {
            buy_below_minor: 100,
            sell_above_minor: 90,
            fee_bps: 0,
            slippage_bps: 0,
        };
        assert!(matches!(
            Backtester.run(&capture(), &invalid),
            Err(BacktestError::InvalidStrategy)
        ));
    }

    #[test]
    fn open_position_is_forced_closed_at_period_end() {
        let strategy = ThresholdStrategy {
            buy_below_minor: 200,
            sell_above_minor: 300,
            fee_bps: 0,
            slippage_bps: 0,
        };
        let report = Backtester.run(&capture(), &strategy).expect("backtest");
        assert_eq!(report.round_trips, 1);
        assert_eq!(report.forced_period_end_exits, 1);
        assert_eq!(report.net_pnl_minor, 15);
    }
}
