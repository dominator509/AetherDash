use std::{env, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `.cargo/config.toml` keeps the Windows Python helper convenient for local
    // development, but that path is not valid on Unix runners.  Probe any
    // configured override and let prost-build discover `protoc` on PATH when
    // the override cannot be executed.
    if let Some(protoc) = env::var_os("PROTOC") {
        let usable = Command::new(&protoc)
            .arg("--version")
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !usable {
            println!(
                "cargo:warning=Ignoring unusable PROTOC override; falling back to protoc on PATH"
            );
            env::remove_var("PROTOC");
        }
    }

    tonic_build::configure().build_server(true).build_client(true).compile_protos(
        &[
            // Core message types (existing)
            "../../proto/aether/core/v1/types.proto",
            "../../proto/aether/core/v1/market_data.proto",
            "../../proto/aether/core/v1/orders.proto",
            "../../proto/aether/core/v1/opportunity.proto",
            // SPEC-003 service contracts (new)
            "../../proto/aether/venue/v1/adapter.proto",
            "../../proto/aether/risk/v1/risk.proto",
            "../../proto/aether/router/v1/router.proto",
            "../../proto/aether/guardian/v1/guardian.proto",
            "../../proto/aether/brain/v1/brain.proto",
        ],
        &["../../proto"],
    )?;
    Ok(())
}
