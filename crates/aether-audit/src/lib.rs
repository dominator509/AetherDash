//! AETHER Audit — tamper-evident append-only log.
//! Every significant action is recorded as a hash-linked chain event.
//! No update or delete API exists by construction.

pub mod chain;
pub mod verifier;
pub mod anchor;
pub mod emission;
pub mod attribution;

pub use chain::{AuditChain, AuditEvent, AuditError, Hash};
pub use verifier::{ChainVerifier, VerificationResult, VerifyError};
pub use anchor::{AnchorStore, Anchor};
pub use emission::ActionClass;
