//! `capsule-observatory`: registry-wide health checks (Phase 8) — the
//! things nobody notices until a stale, contradicted, or overclaiming
//! capsule has already been resolved into an answer.
//!
//! Every check here reads a registry snapshot and reports; none of them
//! mutate anything. Not implemented: source availability tracking (no
//! source has a URL/liveness contract to check yet), delta-update
//! efficiency (there's no delta compilation to measure — see
//! `capsule-compiler`), and evaluation-surface-reduction trend tracking
//! (`capsule-evaluator` doesn't persist past reports anywhere for this
//! to aggregate over).

pub mod error;

pub use error::ObservatoryError;

use capsule_compiler::detect_inflation;
use capsule_core::{LifecycleState, ResolutionStatus};
use capsule_registry::Registry;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessAlert {
    pub logical_id: String,
    pub version: String,
    pub days_since_recorded: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedContradiction {
    pub logical_id: String,
    pub version: String,
    pub target_capsule_id: String,
    pub description: String,
}

/// A published capsule nothing else declares a dependency on. This is
/// informational, not inherently a problem — most leaf-level rules
/// legitimately have zero dependents — but it's the coarse first-pass
/// signal the roadmap asks for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedCapsule {
    pub logical_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenDependency {
    pub logical_id: String,
    pub version: String,
    pub depends_on_logical_id: String,
    pub depends_on_version: Option<String>,
}

/// A capsule whose own statement is inflated relative to its own cited
/// evidence excerpt — confidence laundering caught within a single
/// capsule, not between a source and an answer (that's
/// `capsule-evaluator`'s job).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceLaunderingIncident {
    pub logical_id: String,
    pub version: String,
    pub weak_phrase: String,
    pub inflated_phrase: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDebtEntry {
    pub logical_id: String,
    pub version: String,
    pub lifecycle_state: LifecycleState,
    pub days_since_recorded: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpiryAlert {
    pub logical_id: String,
    pub version: String,
    pub valid_until: String,
}

#[derive(Debug, Clone)]
pub struct ObservatoryOptions {
    /// A published capsule not touched in this many days gets a
    /// freshness alert.
    pub freshness_threshold_days: i64,
    /// A draft/in_review capsule sitting unpublished this long counts
    /// as knowledge debt.
    pub knowledge_debt_threshold_days: i64,
}

impl Default for ObservatoryOptions {
    fn default() -> Self {
        Self { freshness_threshold_days: 180, knowledge_debt_threshold_days: 30 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservatoryReport {
    pub total_capsules_scanned: usize,
    /// `1 - (distinct payloads / total versions)`: the share of
    /// registered versions that share their epistemic payload with
    /// another version — capsule-core's content-addressed dedup made
    /// visible as a single number.
    pub capsule_reuse_ratio: f64,
    pub freshness_alerts: Vec<FreshnessAlert>,
    pub unresolved_contradictions: Vec<UnresolvedContradiction>,
    pub orphaned_capsules: Vec<OrphanedCapsule>,
    pub broken_dependencies: Vec<BrokenDependency>,
    pub confidence_laundering_incidents: Vec<ConfidenceLaunderingIncident>,
    pub knowledge_debt: Vec<KnowledgeDebtEntry>,
    pub expiry_alerts: Vec<ExpiryAlert>,
}

pub fn run(registry: &Registry, options: &ObservatoryOptions) -> Result<ObservatoryReport, ObservatoryError> {
    let now = Utc::now();
    let today = now.date_naive();
    let logical_ids = registry.list_logical_ids()?;

    let mut all_records = Vec::new();
    for id in &logical_ids {
        all_records.extend(registry.list_versions(id)?);
    }

    let mut freshness_alerts = Vec::new();
    let mut unresolved_contradictions = Vec::new();
    let mut broken_dependencies = Vec::new();
    let mut confidence_laundering_incidents = Vec::new();
    let mut knowledge_debt = Vec::new();
    let mut expiry_alerts = Vec::new();

    for record in &all_records {
        let days_since_recorded = parse_rfc3339(&record.recorded_at).map(|t| (now - t).num_days()).unwrap_or(0);

        if record.lifecycle_state == LifecycleState::Published && days_since_recorded >= options.freshness_threshold_days {
            freshness_alerts.push(FreshnessAlert { logical_id: record.logical_id.clone(), version: record.version.clone(), days_since_recorded });
        }

        if matches!(record.lifecycle_state, LifecycleState::Draft | LifecycleState::InReview) && days_since_recorded >= options.knowledge_debt_threshold_days
        {
            knowledge_debt.push(KnowledgeDebtEntry {
                logical_id: record.logical_id.clone(),
                version: record.version.clone(),
                lifecycle_state: record.lifecycle_state,
                days_since_recorded,
            });
        }

        for c in &record.capsule.contradictions {
            if c.resolution_status == ResolutionStatus::Unresolved {
                unresolved_contradictions.push(UnresolvedContradiction {
                    logical_id: record.logical_id.clone(),
                    version: record.version.clone(),
                    target_capsule_id: c.capsule_id.clone(),
                    description: c.description.clone(),
                });
            }
        }

        if record.lifecycle_state == LifecycleState::Published {
            if let Some(valid_until) = record.capsule.applicability.as_ref().and_then(|a| a.valid_until.as_deref()) {
                if NaiveDate::parse_from_str(valid_until, "%Y-%m-%d").map(|d| d < today).unwrap_or(false) {
                    expiry_alerts.push(ExpiryAlert { logical_id: record.logical_id.clone(), version: record.version.clone(), valid_until: valid_until.to_string() });
                }
            }
        }

        for dep in registry.dependencies_of(&record.logical_id, &record.version)? {
            let target_exists = match &dep.depends_on_version {
                Some(v) => registry.get(&dep.depends_on_logical_id, v)?.is_some(),
                None => !registry.list_versions(&dep.depends_on_logical_id)?.is_empty(),
            };
            if !target_exists {
                broken_dependencies.push(BrokenDependency {
                    logical_id: record.logical_id.clone(),
                    version: record.version.clone(),
                    depends_on_logical_id: dep.depends_on_logical_id.clone(),
                    depends_on_version: dep.depends_on_version.clone(),
                });
            }
        }

        for evidence in &record.capsule.evidence {
            let Some(excerpt) = &evidence.excerpt else { continue };
            for finding in detect_inflation(excerpt, &record.capsule.statement) {
                confidence_laundering_incidents.push(ConfidenceLaunderingIncident {
                    logical_id: record.logical_id.clone(),
                    version: record.version.clone(),
                    weak_phrase: finding.weak_phrase,
                    inflated_phrase: finding.inflated_phrase,
                    pattern: finding.pattern.to_string(),
                });
            }
        }
    }

    // Deliberately every *published* version, not `latest_published`:
    // that helper also filters by today's applicability window, which
    // would silently skip an expired-but-still-published capsule here —
    // exactly the kind of capsule this check should not stay quiet
    // about.
    let mut orphaned_capsules = Vec::new();
    for record in all_records.iter().filter(|r| r.lifecycle_state == LifecycleState::Published) {
        if registry.dependents_of(&record.logical_id, &record.version)?.is_empty() {
            orphaned_capsules.push(OrphanedCapsule { logical_id: record.logical_id.clone(), version: record.version.clone() });
        }
    }

    let (payloads, versions) = registry.dedup_stats()?;
    let capsule_reuse_ratio = if versions == 0 { 0.0 } else { 1.0 - (payloads as f64 / versions as f64) };

    Ok(ObservatoryReport {
        total_capsules_scanned: all_records.len(),
        capsule_reuse_ratio,
        freshness_alerts,
        unresolved_contradictions,
        orphaned_capsules,
        broken_dependencies,
        confidence_laundering_incidents,
        knowledge_debt,
        expiry_alerts,
    })
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, LifecycleState as LS};

    fn publish(registry: &mut Registry, json: &str) -> (String, String) {
        let capsule = parse(json).unwrap();
        let record = registry.register(&capsule, "alice").unwrap();
        registry.transition(&record.logical_id, &record.version, LS::InReview, "alice", None).unwrap();
        registry.transition(&record.logical_id, &record.version, LS::Approved, "alice", None).unwrap();
        registry.publish(&record.logical_id, &record.version, "alice").unwrap();
        (record.logical_id, record.version)
    }

    #[test]
    fn detects_unresolved_contradictions() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "fact", "statement": "x", "epistemic_status": "disputed",
          "confidence": {"basis": "unsupported", "ceiling": 0.2}, "lifecycle_state": "draft",
          "contradictions": [{"capsule_id": "kc://a/b/d", "description": "conflict", "resolution_status": "unresolved"}],
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        publish(&mut registry, json);

        let report = run(&registry, &ObservatoryOptions::default()).unwrap();
        assert_eq!(report.unresolved_contradictions.len(), 1);
    }

    #[test]
    fn detects_broken_dependency() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "rule", "statement": "x", "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9}, "lifecycle_state": "draft",
          "dependencies": [{"capsule_id": "kc://a/b/missing@1.0.0", "relation": "requires"}],
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        publish(&mut registry, json);

        let report = run(&registry, &ObservatoryOptions::default()).unwrap();
        assert_eq!(report.broken_dependencies.len(), 1);
        assert_eq!(report.broken_dependencies[0].depends_on_logical_id, "kc://a/b/missing");
    }

    #[test]
    fn detects_orphaned_and_computes_reuse_ratio() {
        let mut registry = Registry::open_in_memory().unwrap();
        let a = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "rule", "statement": "Same claim.", "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9}, "lifecycle_state": "draft",
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        let b = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/d", "version": "1.0.0",
          "capsule_type": "rule", "statement": "Same claim.", "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9}, "lifecycle_state": "draft",
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        publish(&mut registry, a);
        publish(&mut registry, b);

        let report = run(&registry, &ObservatoryOptions::default()).unwrap();
        assert_eq!(report.orphaned_capsules.len(), 2, "neither capsule has a dependent");
        assert_eq!(report.capsule_reuse_ratio, 0.5, "1 payload shared by 2 versions");
    }

    #[test]
    fn detects_expired_but_still_published() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "metric", "statement": "x", "epistemic_status": "estimated",
          "confidence": {"basis": "statistical_inference", "ceiling": 0.5}, "lifecycle_state": "draft",
          "applicability": {"valid_until": "2000-01-01"},
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        publish(&mut registry, json);

        let report = run(&registry, &ObservatoryOptions::default()).unwrap();
        assert_eq!(report.expiry_alerts.len(), 1);
    }

    #[test]
    fn knowledge_debt_flags_stale_drafts_not_fresh_ones() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "fact", "statement": "x", "epistemic_status": "observed",
          "confidence": {"basis": "direct_observation", "ceiling": 0.5}, "lifecycle_state": "draft",
          "evidence": [{"source_id": "s", "authority": "primary"}],
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        let capsule = parse(json).unwrap();
        registry.register(&capsule, "alice").unwrap();

        let report = run(&registry, &ObservatoryOptions { knowledge_debt_threshold_days: 0, ..Default::default() }).unwrap();
        assert_eq!(report.knowledge_debt.len(), 1);

        let report = run(&registry, &ObservatoryOptions { knowledge_debt_threshold_days: 9999, ..Default::default() }).unwrap();
        assert!(report.knowledge_debt.is_empty());
    }

    #[test]
    fn confidence_laundering_incident_detected_within_a_capsule() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
          "capsule_type": "fact", "statement": "The lift was observed at 6%.",
          "epistemic_status": "observed", "confidence": {"basis": "direct_observation", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "evidence": [{"source_id": "s", "excerpt": "The lift was estimated at 6%.", "authority": "primary"}],
          "governance": {"owner": "a", "domain": "d"}
        }"#;
        publish(&mut registry, json);

        let report = run(&registry, &ObservatoryOptions::default()).unwrap();
        assert_eq!(report.confidence_laundering_incidents.len(), 1);
        assert_eq!(report.confidence_laundering_incidents[0].pattern, "estimated -> observed");
    }
}
