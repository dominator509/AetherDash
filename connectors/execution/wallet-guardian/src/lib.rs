//! AETHER Wallet Guardian — isolated signing service.
//!
//! # HARD-DENY invariants
//! 1. No key export, no sign-arbitrary, no message-signing by construction.
//! 2. No LLM/MCP dependencies anywhere in this crate.
//! 3. Withdrawals always require human approval regardless of tier.
//!
//! The Guardian is reached only via gRPC. It is a dependency of nothing.

pub mod broadcast;
pub mod keystore;
pub mod nonce;
pub mod policy;
pub mod proposal;
pub mod rpc;
pub mod service;
pub mod wc;

pub use keystore::KeyStore;
pub use nonce::NonceManager;
pub use policy::{PolicyConfig, PolicyEngine, PolicyResult};
pub use proposal::{CustodyMode, Proposal, ProposalState, ProposalStore, TxSpec};
pub use service::GuardianService;
pub use wc::{PairingClient, PairingUri, WcError, WcSession};
