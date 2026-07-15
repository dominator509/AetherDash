//! WalletConnect v2 integration.
//!
//! In WC mode, the Guardian is the dApp client that assembles
//! transactions and proposes them. The operator's wallet app signs.
//! The policy engine STILL evaluates first — WC is a signer, not a bypass.

pub mod pairing;
pub mod session;

pub use pairing::{
    PairingClient, PairingUri, SessionProposal, WcError, WcTransactionRequest, WcTxParam,
};
pub use session::WcSession;
