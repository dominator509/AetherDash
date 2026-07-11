fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/aether/core/v1/types.proto",
                "../../proto/aether/core/v1/market_data.proto",
                "../../proto/aether/core/v1/orders.proto",
                "../../proto/aether/core/v1/opportunity.proto",
            ],
            &["../../proto"],
        )?;
    Ok(())
}
