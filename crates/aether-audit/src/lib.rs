//! AETHER Audit — tamper-evident append-only log.
//! Every significant action is recorded as a hash-linked chain event.
//! No update or delete API exists by construction.

pub mod anchor;
pub mod attribution;
pub mod chain;
pub mod emission;
pub mod verifier;

pub use anchor::{Anchor, AnchorStore};
pub use chain::{AuditChain, AuditError, AuditEvent, Hash};
pub use emission::ActionClass;
pub use verifier::{ChainVerifier, VerificationResult, VerifyError};
