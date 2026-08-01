//! Core Knowledge Capsule data model (KCP 0.1).
//!
//! Field names mirror `schemas/capsule.schema.json` and
//! `docs/specification/knowledge-capsule-spec.md` exactly so a `Capsule`
//! round-trips through `serde_json` without renames.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "kcp/0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capsule {
    pub schema_version: String,
    pub logical_id: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub capsule_type: CapsuleType,
    pub statement: String,
    pub epistemic_status: EpistemicStatus,
    pub confidence: Confidence,
    pub lifecycle_state: LifecycleState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applicability: Option<Applicability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contradictions: Vec<Contradiction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    pub governance: Governance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_contract: Option<EvaluationContract>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleType {
    Fact,
    Rule,
    Procedure,
    Metric,
    Interpretation,
    Boundary,
    Counterexample,
    Rubric,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicStatus {
    Observed,
    Inferred,
    Estimated,
    Modelled,
    PolicyDefined,
    ExpertJudgement,
    Disputed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBasis {
    DirectObservation,
    StatisticalInference,
    ExpertConsensus,
    ModelPrediction,
    PolicyMandate,
    Heuristic,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confidence {
    pub basis: ConfidenceBasis,
    pub ceiling: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Draft,
    InReview,
    Approved,
    Published,
    Deprecated,
    Superseded,
    Revoked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAuthority {
    Primary,
    Secondary,
    Derived,
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    pub authority: EvidenceAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Applicability {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jurisdictions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Unresolved,
    Resolved,
    Acknowledged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contradiction {
    pub capsule_id: String,
    pub description: String,
    pub resolution_status: ResolutionStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DependencyRelation {
    Requires,
    Refines,
    Supersedes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    pub capsule_id: String,
    pub relation: DependencyRelation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Governance {
    pub owner: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationContract {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mandatory_qualifications: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prohibited_overclaims: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evaluation_rules: Vec<String>,
}
