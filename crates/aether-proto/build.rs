fn main() -> Result<(), Box<dyn std::error::Error>> {
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
