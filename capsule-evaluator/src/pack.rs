//! The set of capsules an answer is evaluated against, with the
//! relationships the detectors need precomputed: supersession,
//! unresolved contradictions, shared evidence sources, and effective
//! confidence ceilings (spec §6.3: a conclusion's confidence cannot exceed
//! the ceiling of its critical dependency).

use crate::text::content_tokens;
use capsule_core::{Capsule, DependencyRelation, LifecycleState, ResolutionStatus};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// `logical_id@version`.
pub fn key(c: &Capsule) -> String {
    format!("{}@{}", c.logical_id, c.version)
}

/// A dependency or contradiction reference may omit the version.
fn matches_ref(reference: &str, c: &Capsule) -> bool {
    reference == c.logical_id || reference == key(c)
}

#[derive(Debug, Clone)]
pub struct PackEntry {
    pub capsule: Capsule,
    pub key: String,
    /// Statement plus applicability conditions, tokenized for mapping.
    pub tokens: BTreeSet<String>,
    pub evidence_sources: BTreeSet<String>,
    /// Ceiling after propagating `requires` dependencies.
    pub effective_ceiling: f64,
    /// The capsule (key) that bounds the ceiling, if not this one.
    pub ceiling_bound_by: Option<String>,
    /// Superseded by a newer capsule in the pack, or not in force.
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CapsulePack {
    pub entries: Vec<PackEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("{path}: {message}")]
    Load { path: String, message: String },
}

impl CapsulePack {
    pub fn new(capsules: Vec<Capsule>) -> Self {
        let mut entries: Vec<PackEntry> = capsules
            .into_iter()
            .map(|c| {
                let mut text = c.statement.clone();
                if let Some(a) = &c.applicability {
                    for cond in &a.conditions {
                        text.push(' ');
                        text.push_str(cond);
                    }
                }
                PackEntry {
                    key: key(&c),
                    tokens: content_tokens(&text),
                    evidence_sources: c.evidence.iter().map(|e| e.source_id.clone()).collect(),
                    effective_ceiling: c.confidence.ceiling,
                    ceiling_bound_by: None,
                    superseded_by: None,
                    capsule: c,
                }
            })
            .collect();

        // Supersession: an explicit `supersedes` dependency, or a newer
        // published version of the same logical id.
        let snapshot: Vec<Capsule> = entries.iter().map(|e| e.capsule.clone()).collect();
        for e in entries.iter_mut() {
            for other in &snapshot {
                if key(other) == e.key {
                    continue;
                }
                let explicit = other
                    .dependencies
                    .iter()
                    .any(|d| d.relation == DependencyRelation::Supersedes && matches_ref(&d.capsule_id, &e.capsule));
                let newer = other.logical_id == e.capsule.logical_id
                    && match (semver::Version::parse(&other.version), semver::Version::parse(&e.capsule.version)) {
                        (Ok(a), Ok(b)) => a > b,
                        _ => false,
                    }
                    && !matches!(other.lifecycle_state, LifecycleState::Revoked | LifecycleState::Draft);
                if explicit || newer {
                    e.superseded_by = Some(key(other));
                }
            }
            if e.superseded_by.is_none() && matches!(e.capsule.lifecycle_state, LifecycleState::Superseded | LifecycleState::Revoked) {
                e.superseded_by = Some(format!("lifecycle:{:?}", e.capsule.lifecycle_state).to_lowercase());
            }
        }

        // Ceiling propagation over `requires`, with cycle protection.
        let ceilings: BTreeMap<String, (f64, Vec<String>)> = snapshot
            .iter()
            .map(|c| {
                let reqs = c
                    .dependencies
                    .iter()
                    .filter(|d| d.relation == DependencyRelation::Requires)
                    .filter_map(|d| snapshot.iter().find(|o| matches_ref(&d.capsule_id, o)).map(key))
                    .collect();
                (key(c), (c.confidence.ceiling, reqs))
            })
            .collect();
        for e in entries.iter_mut() {
            let mut seen = BTreeSet::new();
            let (ceiling, by) = min_ceiling(&e.key, &ceilings, &mut seen);
            e.effective_ceiling = ceiling;
            e.ceiling_bound_by = by.filter(|b| b != &e.key);
        }
        Self { entries }
    }

    /// Loads every `.json` capsule from the given files or directories.
    pub fn load(paths: &[impl AsRef<Path>]) -> Result<Self, PackError> {
        let mut capsules = Vec::new();
        for p in paths {
            let p = p.as_ref();
            let files: Vec<std::path::PathBuf> = if p.is_dir() {
                let mut v: Vec<_> = std::fs::read_dir(p)
                    .map_err(|e| PackError::Load { path: p.display().to_string(), message: e.to_string() })?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|f| f.extension().is_some_and(|x| x == "json"))
                    .collect();
                v.sort();
                v
            } else {
                vec![p.to_path_buf()]
            };
            for f in files {
                let src = std::fs::read_to_string(&f).map_err(|e| PackError::Load { path: f.display().to_string(), message: e.to_string() })?;
                let c = capsule_core::parse(&src).map_err(|e| PackError::Load { path: f.display().to_string(), message: e.to_string() })?;
                capsules.push(c);
            }
        }
        Ok(Self::new(capsules))
    }

    pub fn get(&self, key: &str) -> Option<&PackEntry> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// Capsules with an *unresolved* contradiction with `entry`, in either
    /// direction, with the recorded description.
    pub fn unresolved_contradictions(&self, entry: &PackEntry) -> Vec<(&PackEntry, String)> {
        let mut out = Vec::new();
        for other in &self.entries {
            if other.key == entry.key {
                continue;
            }
            let forward = entry
                .capsule
                .contradictions
                .iter()
                .find(|c| c.resolution_status == ResolutionStatus::Unresolved && matches_ref(&c.capsule_id, &other.capsule));
            let backward = other
                .capsule
                .contradictions
                .iter()
                .find(|c| c.resolution_status == ResolutionStatus::Unresolved && matches_ref(&c.capsule_id, &entry.capsule));
            if let Some(c) = forward.or(backward) {
                out.push((other, c.description.clone()));
            }
        }
        out
    }
}

fn min_ceiling(k: &str, all: &BTreeMap<String, (f64, Vec<String>)>, seen: &mut BTreeSet<String>) -> (f64, Option<String>) {
    let Some((own, reqs)) = all.get(k) else { return (1.0, None) };
    if !seen.insert(k.to_string()) {
        return (*own, Some(k.to_string()));
    }
    let mut best = (*own, Some(k.to_string()));
    for r in reqs {
        let (c, by) = min_ceiling(r, all, seen);
        if c < best.0 {
            best = (c, by);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> Capsule {
        let path = format!("{}/../examples/capsules/{name}.json", env!("CARGO_MANIFEST_DIR"));
        capsule_core::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn links_contradictions_both_ways_and_supersession() {
        let pack = CapsulePack::new(vec![
            example("marketing-incrementality-estimate"),
            example("marketing-incrementality-counterexample"),
            example("data-retention-90-days"),
            example("data-retention-180-days"),
        ]);
        let est = pack.entries.iter().find(|e| e.capsule.logical_id.ends_with("q1-2026")).unwrap();
        let contra = pack.unresolved_contradictions(est);
        assert_eq!(contra.len(), 1);
        assert!(contra[0].0.capsule.logical_id.ends_with("counterexample"));
        let old = pack.entries.iter().find(|e| e.capsule.statement.contains("90 days")).unwrap();
        assert!(old.superseded_by.is_some());
        let new = pack.entries.iter().find(|e| e.capsule.statement.contains("180 days")).unwrap();
        assert!(new.superseded_by.is_none());
    }

    #[test]
    fn ceilings_propagate_through_requires() {
        let mut a = example("marketing-incrementality-estimate");
        let b = example("marketing-incrementality-counterexample");
        a.dependencies.push(capsule_core::Dependency { capsule_id: b.logical_id.clone(), relation: DependencyRelation::Requires });
        let pack = CapsulePack::new(vec![a, b]);
        let e = &pack.entries[0];
        assert_eq!(e.effective_ceiling, 0.3);
        assert!(e.ceiling_bound_by.as_deref().unwrap().contains("counterexample"));
    }
}
