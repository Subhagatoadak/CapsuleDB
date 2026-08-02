use capsule_registry::Registry;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "capsule-api", version, about = "Minimal HTTP JSON API over a capsule-registry")]
struct Cli {
    /// Path to the registry's SQLite database (created if missing).
    #[arg(long, default_value = "capsuledb.sqlite3")]
    db: PathBuf,
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:8089")]
    addr: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "capsule_api=info".into()))
        .init();

    let cli = Cli::parse();
    let registry = Registry::open(&cli.db)?;
    tracing::info!(addr = %cli.addr, db = %cli.db.display(), "capsule-api listening");
    capsule_api::Server::new(registry).run(&cli.addr)?;
    Ok(())
}
