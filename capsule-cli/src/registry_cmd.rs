use crate::load_capsule;
use anyhow::{Context, Result};
use capsule_core::LifecycleState;
use capsule_registry::{CapsuleRecord, Registry};
use clap::Subcommand;
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
    }

    Ok(())
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
