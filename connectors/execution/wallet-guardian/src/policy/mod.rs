//! Policy evaluation engine — allowlist, simulation, limits, routing, gas checks.

pub mod allowlist;
pub mod engine;
pub mod limits;
pub mod simulation;

pub use engine::{PolicyConfig, PolicyEngine, PolicyResult};
pub use simulation::{local_validate, simulate, simulate_async, RpcSimulator, SimulationResult};
