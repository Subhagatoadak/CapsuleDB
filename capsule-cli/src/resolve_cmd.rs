use anyhow::{bail, Context, Result};
use capsule_registry::Registry;
use capsule_resolver::resolve::ResolutionBundle;
use capsule_resolver::{parse_cql, CapsuleSource, PackSource, RegistrySource, ResolutionRequest, Resolver};
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ResolveArgs {
    /// A CQL `RESOLVE` query: inline text, or a path to a .cql file.
    #[arg(long, conflicts_with = "request")]
    pub cql: Option<String>,
    /// A JSON resolution request (KCP §5).
    #[arg(long)]
    pub request: Option<PathBuf>,
    /// Capsule JSON files or directories.
    #[arg(long, num_args = 1.., conflicts_with = "registry")]
    pub capsules: Vec<PathBuf>,
    /// A capsule-registry SQLite database.
    #[arg(long)]
    pub registry: Option<PathBuf>,
    /// Replay a recorded bundle (JSON) against the current source.
    #[arg(long)]
    pub replay: Option<PathBuf>,
    /// Date used when the request has no AS OF (default: today, UTC).
    #[arg(long)]
    pub today: Option<String>,
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: ResolveArgs) -> Result<()> {
    let registry = match &args.registry {
        Some(p) => Some(Registry::open(p).with_context(|| format!("opening {}", p.display()))?),
        None => None,
    };
    let pack;
    let source: &dyn CapsuleSource = match &registry {
        Some(r) => &RegistrySource(r),
        None if !args.capsules.is_empty() => {
            pack = PackSource::load(&args.capsules).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            &pack
        }
        None => bail!("give --capsules or --registry"),
    };
    let today = args.today.clone().unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let resolver = Resolver::new(&today);

    if let Some(path) = &args.replay {
        let recorded: ResolutionBundle = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let r = resolver.replay(&recorded, source).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&r)?);
        } else {
            println!("identical: {}   source changed: {}", r.identical, r.source_changed);
            for d in &r.differences {
                println!("  {d}");
            }
        }
        if !r.identical {
            std::process::exit(3);
        }
        return Ok(());
    }

    let req: ResolutionRequest = match (&args.cql, &args.request) {
        (Some(q), _) => {
            let text = if std::path::Path::new(q).is_file() { std::fs::read_to_string(q)? } else { q.clone() };
            parse_cql(&text)?
        }
        (None, Some(p)) => serde_json::from_str(&std::fs::read_to_string(p)?)?,
        (None, None) => bail!("give --cql or --request"),
    };
    let b = resolver.resolve(&req, source).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&b)?);
        return Ok(());
    }
    println!("as of {}   planned types: {:?}", b.as_of, b.planned_types);
    println!("\nresolved:");
    for r in &b.resolved {
        println!("  {}. {:?} {}  (ceiling {:.2})", r.rank, r.role, r.key, r.effective_ceiling);
        println!("     {}", r.capsule.statement);
        if !r.conditions_unverified.is_empty() {
            println!("     conditions not established: {}", r.conditions_unverified.join("; "));
        }
    }
    for c in &b.contradictions {
        println!("\ncontradiction: {}\n  {}", c.capsules.join(" <-> "), c.description);
    }
    if !b.known_gaps.is_empty() {
        println!("\nknown gaps:");
        for g in &b.known_gaps {
            println!("  {}: {}", g.kind, g.detail);
        }
    }
    println!("\nplan:");
    for p in &b.plan {
        println!("  [{}] {} {}: {}", p.step, p.decision, p.capsule, p.reason);
    }
    println!("\nresult digest {}", b.result_digest);
    Ok(())
}
