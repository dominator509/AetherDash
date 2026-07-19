//! AETHER Terminal — Testing infrastructure: deterministic replay harness,
//! lifecycle assertion framework, and regression test infrastructure.
//!
//! # Modules
//!
//! - **replay**: capture, persist, replay, and verify bus message sequences
//! - **lifecycle**: state-machine validation for opportunity lifecycle (SPEC-012)
//! - **regression**: golden-based regression detection across versions

pub mod lifecycle;
pub mod regression;
pub mod replay;

pub use lifecycle::{LifecycleAssertion, LifecycleChecker, TransitionTrace};
pub use regression::{RegressionCase, RegressionResult, RegressionSuite};
pub use replay::{CapturedEvent, ReplayError, ReplayHarness, ReplayResult};
