//! Capsule Packs: a named, versioned bundle of capsule references — the
//! unit the README's Capsule Query Language sketches as `VERIFY answer
//! AGAINST PACK "marketing-incrementality"`. Distinct from a `.kcap`
//! package (one signed, self-contained capsule archive): a pack is a
//! lightweight manifest of *references* into a registry, not a copy of
//! the capsules themselves.

use anyhow::{bail, Context, Result};
use capsule_core::LifecycleState;
use capsule_registry::Registry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
struct CapsulePack {
    name: String,
    #[serde(default)]
    description: String,
    /// Each entry is `kc://domain/topic/name` (resolved to the latest
    /// published version when the pack is used) or
    /// `kc://domain/topic/name@1.2.3` (pinned).
    members: Vec<String>,
}

pub fn create(output: &Path, name: &str, description: Option<&str>, members: Vec<String>) -> Result<()> {
    if members.is_empty() {
        bail!("a pack needs at least one member");
    }
    let pack = CapsulePack { name: name.to_string(), description: description.unwrap_or_default().to_string(), members };
    let json = serde_json::to_string_pretty(&pack)?;
    fs::write(output, json).with_context(|| format!("writing {}", output.display()))?;
    println!("created pack '{}' with {} member(s) -> {}", pack.name, pack.members.len(), output.display());
    Ok(())
}

pub fn resolve(pack_file: &Path, db: &Path) -> Result<()> {
    let json = fs::read_to_string(pack_file).with_context(|| format!("reading {}", pack_file.display()))?;
    let pack: CapsulePack = serde_json::from_str(&json).with_context(|| format!("parsing pack at {}", pack_file.display()))?;
    let registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;

    println!("pack: {} — {}", pack.name, pack.description);
    println!("{} member(s):\n", pack.members.len());

    let mut all_published = true;
    for member in &pack.members {
        let (logical_id, version) = match member.rsplit_once('@') {
            Some((id, v)) => (id.to_string(), Some(v.to_string())),
            None => (member.clone(), None),
        };

        let record = match &version {
            Some(v) => registry.get(&logical_id, v)?,
            None => registry.latest_published(&logical_id, None)?,
        };

        match record {
            Some(record) => {
                let published = record.lifecycle_state == LifecycleState::Published;
                all_published &= published;
                let mark = if published { "OK" } else { "!!" };
                println!("  [{mark}] {}@{} [{:?}]", record.logical_id, record.version, record.lifecycle_state);
                println!("        {}", record.capsule.statement);
            }
            None => {
                all_published = false;
                println!("  [!!] {member}: not found in registry");
            }
        }
    }

    println!();
    if all_published {
        println!("all members resolved and published.");
        Ok(())
    } else {
        bail!("one or more pack members are missing or not published");
    }
}
