//! AETHER Audit Verify CLI.
//! Usage: aether-audit verify [--incremental|--full]

use aether_audit::anchor::AnchorStore;
use aether_audit::chain::AuditChain;
use aether_audit::verifier::ChainVerifier;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(2).map(|s| s.as_str()).unwrap_or("--full");

    let mut chain = AuditChain::new();
    // In production: load chain from Postgres/ClickHouse
    if let Err(error) = chain.append("cli", "verify.run", "audit", b"") {
        eprintln!("verify: FAILED");
        eprintln!("  error: {error}");
        process::exit(1);
    }

    let result = match mode {
        "--incremental" => {
            let anchor_store = AnchorStore::new(10);
            ChainVerifier::verify_incremental(&chain, &anchor_store)
        }
        _ => ChainVerifier::verify_full(&chain),
    };

    match result {
        Ok(r) => {
            println!("verify: ok");
            println!("  events_checked: {}", r.events_checked);
            println!("  from_seq: {}", r.from_seq);
            println!("  to_seq: {}", r.to_seq);
            println!("  incremental: {}", r.is_incremental);
            process::exit(0);
        }
        Err(e) => {
            eprintln!("verify: FAILED");
            eprintln!("  error: {}", e);
            process::exit(1);
        }
    }
}
