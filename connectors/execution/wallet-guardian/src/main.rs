//! AETHER Wallet Guardian — binary entry point.
fn main() {
    println!("AETHER Wallet Guardian v{}", env!("CARGO_PKG_VERSION"));
    println!("HARD-DENY: No key export, no sign-arbitrary, no message-signing.");
    println!("HARD-DENY: Withdrawals always require human approval.");
}
