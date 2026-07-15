//! Net-edge decomposition engine per SPEC-012.
//! All 11 components implemented as pure functions.

pub mod components;
pub mod decompose;
pub mod mismatch;

pub use components::*;
pub use decompose::{decompose, DecompositionContext, EdgeDecomposition};
pub use mismatch::MismatchConfig;
