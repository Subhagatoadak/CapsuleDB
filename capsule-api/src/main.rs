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
    let cli = Cli::parse();
    let registry = Registry::open(&cli.db)?;
    println!("capsule-api listening on http://{} (db: {})", cli.addr, cli.db.display());
    capsule_api::Server::new(registry).run(&cli.addr)?;
    Ok(())
}
