//! AETHER Simulator: net-edge decomposition + fill walk + sensitivity.
//!
//! Computes full 11-component decompositions at request time.
//! Uses the shared aether-fillmodel for fills — parity with paper ledger
//! is REQUIRED per SPEC-012.

pub mod sensitivity;
pub mod simulator;

pub use sensitivity::SensitivityTable;
pub use simulator::{
    walk_leg, Simulation, SimulationConfig, SimulationError, SimulationInput, Simulator,
};
