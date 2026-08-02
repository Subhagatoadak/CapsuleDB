//! The resolution request/response contract, following
//! `protocol/KCP-v0.1.md` §5.

use capsule_core::{Capsule, CapsuleType, LifecycleState};
use capsule_registry::CapsuleRecord;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionRequest {
    pub purpose: String,
    /// Restricts to logical IDs under `kc://<domain>/...`.
    #[serde(default)]
    pub domain: Option<String>,
    /// Case-insensitive substring match against the capsule statement —
    /// the "lexical discovery index" for this MVP (no semantic/vector
    /// index yet).
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Resolve as of this date; defaults to today (temporal version
    /// resolution, spec §6.3 / registry `latest_published`).
    #[serde(default)]
    pub as_of: Option<NaiveDate>,
    #[serde(default)]
    pub capsule_types: Option<Vec<CapsuleType>>,
    /// If set, a capsule is only included when its `governance.permissions`
    /// is empty (public) or contains this role (identity/permission
    /// filtering).
    #[serde(default)]
    pub consumer_role: Option<String>,
    /// Evidence-sufficiency gate: exclude candidates whose evidence is
    /// entirely `unverified`. Defaults to `false` so compiler-produced
    /// drafts remain resolvable before human review upgrades them.
    #[serde(default)]
    pub require_sufficient_evidence: bool,
    /// Include the contradiction bundle (capsules that contradict a
    /// resolved capsule) in the response. Defaults to `true`.
    #[serde(default = "default_true")]
    pub include_contradictions: bool,
    /// Include transitively `requires`d dependencies in the response.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub include_dependencies: bool,
}

fn default_true() -> bool {
    true
}

impl ResolutionRequest {
    pub fn new(purpose: impl Into<String>) -> Self {
        Self {
            purpose: purpose.into(),
            domain: None,
            about: None,
            jurisdiction: None,
            as_of: None,
            capsule_types: None,
            consumer_role: None,
            require_sufficient_evidence: false,
            include_contradictions: true,
            include_dependencies: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncludedAs {
    /// Matched the request's own filters.
    Primary,
    /// Pulled in to complete a `requires` dependency of a primary result.
    Dependency,
    /// Pulled in because it contradicts a primary or dependency result.
    Contradiction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCapsule {
    pub logical_id: String,
    pub version: String,
    pub lifecycle_state: LifecycleState,
    pub capsule: Capsule,
    pub included_as: IncludedAs,
    /// The confidence ceiling this capsule may contribute after applying
    /// spec §6.3: "the confidence of a conclusion cannot exceed the
    /// confidence ceiling of its critical unsupported dependency." Equal
    /// to `capsule.confidence.ceiling` unless a `requires` dependency
    /// (transitively) has a lower one.
    pub effective_confidence_ceiling: f64,
}

impl From<(CapsuleRecord, IncludedAs, f64)> for ResolvedCapsule {
    fn from((record, included_as, effective_confidence_ceiling): (CapsuleRecord, IncludedAs, f64)) -> Self {
        Self {
            logical_id: record.logical_id,
            version: record.version,
            lifecycle_state: record.lifecycle_state,
            capsule: record.capsule,
            included_as,
            effective_confidence_ceiling,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Included,
    Excluded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub logical_id: String,
    pub version: Option<String>,
    pub decision: Decision,
    pub reason: String,
}

/// A capsule (or dependency reference) the resolver could not fully
/// satisfy: a `requires` dependency missing from the registry, or one
/// excluded by the evidence-sufficiency gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownGap {
    pub logical_id: String,
    pub version: Option<String>,
    pub reason: String,
}

/// A contradiction surfaced during resolution: the capsule that declared
/// it, and — if it resolved to a registered capsule — the capsule it
/// contradicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionBundleEntry {
    pub declaring_logical_id: String,
    pub declaring_version: String,
    pub target_capsule_id: String,
    pub target_resolved: bool,
    pub description: String,
}

/// The full result of a resolution: the resolved capsule set, the
/// contradiction bundle, known gaps, and an inspectable, serializable
/// plan explaining every inclusion/exclusion decision. Serializing a
/// `ResolutionBundle` and later calling
/// [`crate::Resolver::replay`] on its `request` re-runs the same
/// resolution against the registry's current state — "replayable
/// resolution."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionBundle {
    pub request: ResolutionRequest,
    pub capsules: Vec<ResolvedCapsule>,
    pub contradictions: Vec<ContradictionBundleEntry>,
    pub gaps: Vec<KnownGap>,
    pub plan: Vec<PlanStep>,
}
