#![allow(clippy::print_stdout)]

use aether_plugin::{GeneratedPluginDraft, PgPluginRepository};
use sqlx::PgPool;
use std::io::Read;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("plugin submission failed: {}", error.code());
        std::process::exit(1);
    }
}

async fn run() -> Result<(), SubmitError> {
    let mut input = String::new();
    std::io::stdin().take(300_000).read_to_string(&mut input)?;
    let draft: GeneratedPluginDraft = serde_json::from_str(&input)?;
    let compiled = draft.compile()?;
    let database_url = std::env::var("DATABASE_URL").map_err(|_| SubmitError::Configuration)?;
    let pool = PgPool::connect(&database_url).await?;
    PgPluginRepository::new(pool).install_generated(&compiled).await?;
    println!(
        "{}",
        serde_json::json!({
            "name": compiled.manifest.name,
            "version": compiled.manifest.version,
            "status": "installed"
        })
    );
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum SubmitError {
    #[error("configuration unavailable")]
    Configuration,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Compile(#[from] aether_plugin::GenerationCompileError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl SubmitError {
    const fn code(&self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Io(_) => "input",
            Self::Json(_) => "invalid_draft",
            Self::Compile(_) => "compile_denied",
            Self::Database(_) => "database",
        }
    }
}
