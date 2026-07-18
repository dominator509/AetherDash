//! AETHER Wallet Guardian — isolated durable gRPC service.

use aether_proto::aether::guardian::v1::wallet_guardian_server::WalletGuardianServer;
use aether_wallet_guardian::grpc::GuardianGrpc;
use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::totp::CredentialTotpAuthority;
use aether_wallet_guardian::worker::BroadcastWorker;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tonic::transport::Server;
use zeroize::Zeroize;

#[derive(Debug, Error)]
enum StartupError {
    #[error("Guardian configuration is invalid")]
    Configuration,
    #[error("Guardian keystore is unavailable")]
    Keystore,
    #[error("Guardian persistence is unavailable")]
    Persistence,
    #[error("Guardian gRPC service could not start")]
    Transport,
    #[error("Guardian broadcast worker stopped unexpectedly")]
    Worker,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();
    if let Err(error) = run().await {
        tracing::error!(error = %error, "wallet Guardian startup failed");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), StartupError> {
    let bind: SocketAddr = std::env::var("AETHER_GUARDIAN__BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50053".into())
        .parse()
        .map_err(|_| StartupError::Configuration)?;
    if !bind.ip().is_loopback() {
        return Err(StartupError::Configuration);
    }
    let mut database_url =
        std::env::var("DATABASE_URL").map_err(|_| StartupError::Configuration)?;
    let pool_result =
        PgPoolOptions::new().min_connections(1).max_connections(5).connect(&database_url).await;
    database_url.zeroize();
    let pool = pool_result.map_err(|_| StartupError::Persistence)?;
    let keystore = Arc::new(KeyStore::from_env().map_err(|_| StartupError::Keystore)?);
    let totp = Arc::new(CredentialTotpAuthority::from_env().map_err(|_| StartupError::Keystore)?);
    let guardian = GuardianGrpc::from_env_shared(pool.clone(), keystore.clone(), totp)
        .map_err(|_| StartupError::Configuration)?;
    let worker =
        BroadcastWorker::from_env(pool, keystore).map_err(|_| StartupError::Configuration)?;
    let (mut health, health_service) = tonic_health::server::health_reporter();
    health.set_serving::<WalletGuardianServer<GuardianGrpc>>().await;
    tracing::info!(bind = %bind, "wallet Guardian gRPC service ready");
    let server = Server::builder()
        .add_service(health_service)
        .add_service(WalletGuardianServer::new(guardian))
        .serve(bind);
    tokio::select! {
        result = server => result.map_err(|_| StartupError::Transport),
        _ = worker.run() => Err(StartupError::Worker),
    }
}
