//! Shared book-walk fill model.
//!
//! This crate implements the deterministic fill algorithm used by both
//! the paper ledger (EP-304) and the simulator (EP-307).  The fill model
//! is pure: given the same book, intent, and config it always produces
//! the same fills.

pub mod config;
pub mod walk;

pub use aether_core::order::Fill;
pub use config::{Aggressiveness, FillConfig};
pub use walk::{walk, walk_book, FillError};
