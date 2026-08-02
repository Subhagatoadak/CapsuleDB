use crate::load_capsule;
use anyhow::{Context, Result};
use capsule_core::LifecycleState;
use capsule_registry::{CapsuleRecord, Registry};
use clap::Subcommand;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum RegistryCommand {
    /// Register a draft capsule document in the registry.
    Register {
        capsule: PathBuf,
        #[arg(long)]
        actor: String,
    },
    /// Apply a lifecycle transition (e.g. draft -> in_review -> approved).
    Transition {
        logical_id: String,
        version: String,
        /// Target lifecycle state: draft, in_review, approved, published, deprecated, superseded, revoked.
        to: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Publish an approved capsule; auto-supersedes any `supersedes` dependency target.
    Publish {
        logical_id: String,
        version: String,
        #[arg(long)]
        actor: String,
    },
    /// Fetch one registered capsule version.
    Get { logical_id: String, version: String },
    /// List every registered version of a logical ID, newest first.
    List { logical_id: String },
    /// Show the lifecycle ledger (approval/publication/revocation history) for one version.
    History { logical_id: String, version: String },
    /// Show what one capsule version depends on.
    Dependencies { logical_id: String, version: String },
    /// Show what depends on one capsule version.
    Dependents { logical_id: String, version: String },
    /// Show every capsule citing a given evidence source (an evidence family).
    Evidence { source_id: String },
    /// Show the most recent registry audit events.
    Audit {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Export every registered capsule version to a JSONL file (one capsule document per line).
    Export {
        #[arg(long)]
        out: PathBuf,
    },
    /// Bulk-register capsules from a JSONL file (as produced by `export`), replaying each one's lifecycle state.
    Import {
        file: PathBuf,
        /// Actor recorded for the registration and any replayed lifecycle transitions.
        #[arg(long)]
        actor: String,
    },
}

pub fn run(db: &Path, action: RegistryCommand) -> Result<()> {
    let mut registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;

    match action {
        RegistryCommand::Register { capsule, actor } => {
            let capsule = load_capsule(&capsule)?;
            let record = registry.register(&capsule, &actor)?;
            println!("registered {}@{} [{:?}] content_hash={}", record.logical_id, record.version, record.lifecycle_state, record.content_hash);
        }
        RegistryCommand::Transition { logical_id, version, to, actor, reason } => {
            let to = parse_lifecycle_state(&to)?;
            registry.transition(&logical_id, &version, to, &actor, reason.as_deref())?;
            println!("{logical_id}@{version} -> {to:?}");
        }
        RegistryCommand::Publish { logical_id, version, actor } => {
            let outcomes = registry.publish(&logical_id, &version, &actor)?;
            println!("{logical_id}@{version} -> Published");
            for outcome in outcomes {
                let mark = if outcome.applied { "superseded" } else { "not superseded" };
                println!("  {} {}@{}: {} ({})", mark, outcome.target_logical_id, outcome.target_version, outcome.note, outcome.applied);
            }
        }
        RegistryCommand::Get { logical_id, version } => match registry.get(&logical_id, &version)? {
            Some(record) => print_record(&record),
            None => println!("not found: {logical_id}@{version}"),
        },
        RegistryCommand::List { logical_id } => {
            let versions = registry.list_versions(&logical_id)?;
            if versions.is_empty() {
                println!("no versions registered for {logical_id}");
            }
            for record in versions {
                println!("{}@{} [{:?}] content_hash={}", record.logical_id, record.version, record.lifecycle_state, record.content_hash);
            }
        }
        RegistryCommand::History { logical_id, version } => {
            for event in registry.history(&logical_id, &version)? {
                let from = event.from_state.map(|s| format!("{s:?}")).unwrap_or_else(|| "-".to_string());
                let reason = event.reason.map(|r| format!(" ({r})")).unwrap_or_default();
                println!("{} {from} -> {:?} by {}{reason}", event.occurred_at, event.to_state, event.actor);
            }
        }
        RegistryCommand::Dependencies { logical_id, version } => {
            for dep in registry.dependencies_of(&logical_id, &version)? {
                let target_version = dep.depends_on_version.as_deref().unwrap_or("*");
                println!("{:?} {}@{}", dep.relation, dep.depends_on_logical_id, target_version);
            }
        }
        RegistryCommand::Dependents { logical_id, version } => {
            for dep in registry.dependents_of(&logical_id, &version)? {
                println!("{}@{} ({:?})", dep.logical_id, dep.version, dep.relation);
            }
        }
        RegistryCommand::Evidence { source_id } => {
            for link in registry.evidence_family(&source_id)? {
                println!("{}@{} [{:?}]", link.logical_id, link.version, link.authority);
            }
        }
        RegistryCommand::Audit { limit } => {
            for event in registry.audit_log(limit)? {
                let logical_id = event.logical_id.unwrap_or_else(|| "-".to_string());
                let version = event.version.unwrap_or_else(|| "-".to_string());
                println!("{} {} {}@{} by {}", event.occurred_at, event.event_type, logical_id, version, event.actor);
            }
        }
        RegistryCommand::Export { out } => {
            let mut file = fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?;
            let mut count = 0usize;
            for logical_id in registry.list_logical_ids()? {
                for record in registry.list_versions(&logical_id)? {
                    writeln!(file, "{}", serde_json::to_string(&record.capsule)?)?;
                    count += 1;
                }
            }
            println!("exported {count} capsule version(s) -> {}", out.display());
        }
        RegistryCommand::Import { file, actor } => {
            let reader = BufReader::new(fs::File::open(&file).with_context(|| format!("opening {}", file.display()))?);
            let (mut imported, mut skipped, mut failed) = (0usize, 0usize, 0usize);
            for (line_number, line) in reader.lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                match import_one(&mut registry, &line, &actor) {
                    Ok(true) => imported += 1,
                    Ok(false) => skipped += 1,
                    Err(e) => {
                        eprintln!("line {}: {e}", line_number + 1);
                        failed += 1;
                    }
                }
            }
            println!("imported {imported}, skipped (already present) {skipped}, failed {failed}");
        }
    }

    Ok(())
}

/// Registers one exported capsule document, forcing it to `draft` first
/// (lifecycle state is administrative and excluded from the content
/// hash — see `capsule-core::hashing` — so this doesn't change identity)
/// and then replaying the minimal transition chain to reach its
/// original lifecycle state. Returns `Ok(true)` if newly imported,
/// `Ok(false)` if it was already present with identical content.
fn import_one(registry: &mut Registry, line: &str, actor: &str) -> Result<bool> {
    let mut value: serde_json::Value = serde_json::from_str(line)?;
    let original_state: LifecycleState = serde_json::from_value(value["lifecycle_state"].clone())?;
    value["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_capsule: capsule_core::Capsule = serde_json::from_value(value)?;

    let logical_id = draft_capsule.logical_id.clone();
    let version = draft_capsule.version.clone();
    let already_present = registry.get(&logical_id, &version)?.is_some();

    registry.register(&draft_capsule, actor)?;

    use LifecycleState::*;
    let path: &[LifecycleState] = match original_state {
        Draft => &[],
        InReview => &[InReview],
        Approved => &[InReview, Approved],
        Published => &[InReview, Approved, Published],
        Deprecated => &[InReview, Approved, Published, Deprecated],
        Superseded => &[InReview, Approved, Published, Superseded],
        Revoked => &[Revoked],
    };
    for state in path {
        // Already at or past this state (e.g. re-importing into the same
        // registry) — a repeat transition would be rejected as illegal.
        if registry.get(&logical_id, &version)?.map(|r| r.lifecycle_state) == Some(*state) {
            continue;
        }
        registry.transition(&logical_id, &version, *state, actor, Some("bulk import replay"))?;
    }

    Ok(!already_present)
}

fn print_record(record: &CapsuleRecord) {
    println!("logical_id     : {}", record.logical_id);
    println!("version        : {}", record.version);
    println!("lifecycle_state: {:?}", record.lifecycle_state);
    println!("content_hash   : {}", record.content_hash);
    println!("recorded_at    : {}", record.recorded_at);
    println!("statement      : {}", record.capsule.statement);
}

fn parse_lifecycle_state(s: &str) -> Result<LifecycleState> {
    let value = serde_json::Value::String(s.to_lowercase());
    serde_json::from_value(value).with_context(|| {
        format!("'{s}' is not a lifecycle state (expected one of: draft, in_review, approved, published, deprecated, superseded, revoked)")
    })
}
