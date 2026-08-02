use capsule_core::{Capsule, DependencyRelation, EvidenceAuthority, LifecycleState};
use serde::Serialize;
use serde_json::Value;

/// A registered capsule version, with its current (derived) lifecycle
/// state and the fully reconstructed document.
#[derive(Debug, Clone)]
pub struct CapsuleRecord {
    pub logical_id: String,
    pub version: String,
    pub content_hash: String,
    pub lifecycle_state: LifecycleState,
    pub recorded_at: String,
    pub capsule: Capsule,
}

/// One entry in a capsule version's append-only lifecycle ledger. This
/// doubles as the approval, publication, and revocation record: `to_state
/// == Approved` is the approval record, `Published` the publication
/// record, `Revoked` the revocation record, and so on.
#[derive(Debug, Clone)]
pub struct LifecycleEvent {
    pub logical_id: String,
    pub version: String,
    pub from_state: Option<LifecycleState>,
    pub to_state: LifecycleState,
    pub actor: String,
    pub reason: Option<String>,
    pub occurred_at: String,
}

#[derive(Debug, Clone)]
pub struct DependencyRecord {
    pub logical_id: String,
    pub version: String,
    pub depends_on_logical_id: String,
    pub depends_on_version: Option<String>,
    pub relation: DependencyRelation,
    pub recorded_at: String,
}

/// One capsule's citation of an evidence source. Grouping these by
/// `source_id` across capsules is the "evidence-family tracking" the
/// roadmap calls for: it shows every capsule that draws on the same
/// underlying source, which is what Phase 6's evidence-multiplication
/// detector will need.
#[derive(Debug, Clone)]
pub struct EvidenceLink {
    pub logical_id: String,
    pub version: String,
    pub source_id: String,
    pub authority: EvidenceAuthority,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub event_type: String,
    pub logical_id: Option<String>,
    pub version: Option<String>,
    pub actor: String,
    pub detail: Option<Value>,
    pub occurred_at: String,
}

/// Outcome of an automatic supersession attempted while publishing a
/// capsule that declares a `supersedes` dependency (spec §8).
#[derive(Debug, Clone)]
pub struct SupersessionOutcome {
    pub target_logical_id: String,
    pub target_version: String,
    pub applied: bool,
    pub note: String,
}
