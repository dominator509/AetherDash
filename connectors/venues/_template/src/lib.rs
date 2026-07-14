//! AETHER Terminal -- {VENUE_NAME} venue adapter library (template).
//!
//! TODO: Replace {VENUE_NAME} with the actual venue display name.
//! When you rename this crate, remove the `non_snake_case` allow below.

#![allow(non_snake_case)]
//! See SPEC-009 for the venue adapter contract and ARCHITECTURE.md §13.
//!
//! # Module structure
//!
//! - `auth` -- Venue-specific authentication / signing
//! - `normalize` -- Market data normalization to canonical `aether_core` types
//! - `orders` -- Order placement and management (TODO)
//! - `health` -- Venue health checks (TODO)

pub mod auth;
pub mod normalize;

// TODO: uncomment as modules are implemented
// pub mod orders;
// pub mod health;

// Re-exports for external consumers
pub use auth::VenueAuth;
pub use normalize::normalize_market;
