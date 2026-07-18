//! AETHER Simulator — CLI entry point.
//!
//! Reads one `SimulationInput` JSON document from stdin and writes one
//! `Simulation` JSON document to stdout. This provides a stable transport
//! seam for the MCP/gateway adapter without duplicating edge math in Python.

use aether_simulator::{SimulationConfig, SimulationInput, Simulator};
use std::io::{self, Read};

fn main() {
    if let Err(error) = run() {
        eprintln!("simulation failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input_json = String::new();
    io::stdin().read_to_string(&mut input_json)?;
    let input: SimulationInput = serde_json::from_str(&input_json)?;
    let simulator = Simulator::new(SimulationConfig::default())?;
    let result = simulator.simulate(&input)?;
    serde_json::to_writer(io::stdout().lock(), &result)?;
    Ok(())
}
