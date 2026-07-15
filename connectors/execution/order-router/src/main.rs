//! AETHER Order Router -- binary entry point.
//!
//! The order router is typically consumed as a library by the server plane.
//! This binary exists for standalone smoke tests and diagnostics.

use aether_order_router::router::RouterConfig;

fn main() {
    let config = RouterConfig::default();
    println!("AETHER Order Router v{}", env!("CARGO_PKG_VERSION"));
    println!("  max_concurrent_orders: {}", config.max_concurrent_orders);
    println!("Router ready.");
}
