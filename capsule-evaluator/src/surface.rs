//! Evaluation-surface manifests: a reviewed, versioned description of how
//! answers about a capsule pack are evaluated — which deterministic checks
//! run, their thresholds, extra deterministic rules, and exactly which
//! residual questions a judge may be asked, at what discretion level.
//!
//! The point is to shrink what a judge decides. A judge never scores an
//! answer; it may only answer a bounded question about one claim and one
//! capsule, choosing from fixed labels.

use serde::{Deserialize, Serialize};

/// How much latitude an LLM judge has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JudgeDiscretion {
    /// No judge: residual questions stay unresolved and are reported.
    #[default]
    None,
    /// The judge picks one label from a fixed set; no rationale is used.
    Classify,
    /// As `Classify`, plus a short rationale recorded for reviewers.
    ClassifyWithRationale,
}

/// Residual question kinds a manifest may refer to a judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualKind {
    /// Does this capsule support this claim? (lexical mapping was ambiguous)
    ClaimSupport,
    /// Does the answer convey this mandatory qualification? (partial match)
    QualificationConveyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    UnsupportedClaims,
    ApplicabilityExpansion,
    InferenceToFact,
    AssociationToCausation,
    ConfidenceCeiling,
    MissingQualifications,
    ContradictionSuppression,
    EvidenceMultiplication,
    ProhibitedOverclaims,
    SupersededKnowledge,
    Rules,
}

impl Check {
    pub const ALL: [Check; 11] = [
        Check::UnsupportedClaims,
        Check::ApplicabilityExpansion,
        Check::InferenceToFact,
        Check::AssociationToCausation,
        Check::ConfidenceCeiling,
        Check::MissingQualifications,
        Check::ContradictionSuppression,
        Check::EvidenceMultiplication,
        Check::ProhibitedOverclaims,
        Check::SupersededKnowledge,
        Check::Rules,
    ];
}

/// A deterministic rule. Phrases are matched case-insensitively.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub description: String,
    /// Only applies when the answer relies on this capsule (logical id
    /// or `id@version`). Absent: always applies.
    #[serde(default)]
    pub when_capsule: Option<String>,
    /// The answer must not contain any of these.
    #[serde(default)]
    pub forbid: Vec<String>,
    /// The answer must contain at least one of these.
    #[serde(default)]
    pub require_any: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    /// Mapping score at or above which a capsule supports a claim.
    pub support: f64,
    /// Mapping score at or above which a claim is about a capsule at all
    /// (between this and `support`, support is a residual question).
    pub related: f64,
    /// Token coverage at or above which a qualification counts as conveyed.
    pub qualification: f64,
    /// Coverage below which a qualification is clearly missing (between
    /// this and `qualification` it is a residual question).
    pub qualification_floor: f64,
    /// Similarity at or above which a claim restates a prohibited overclaim.
    pub overclaim: f64,
    /// Tolerance above the ceiling before a confidence claim is a violation.
    pub ceiling_tolerance: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { support: 0.5, related: 0.3, qualification: 0.5, qualification_floor: 0.25, overclaim: 0.6, ceiling_tolerance: 0.05 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceManifest {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Checks to run (default: all).
    #[serde(default = "all_checks")]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub judge: JudgePolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgePolicy {
    #[serde(default)]
    pub discretion: JudgeDiscretion,
    /// Residual kinds the judge may be asked (default: none).
    #[serde(default)]
    pub may_answer: Vec<ResidualKind>,
    /// Hard cap on judge calls per evaluation.
    #[serde(default = "default_max_calls")]
    pub max_calls: usize,
}

impl Default for JudgePolicy {
    fn default() -> Self {
        Self { discretion: JudgeDiscretion::None, may_answer: vec![], max_calls: default_max_calls() }
    }
}

fn default_version() -> String {
    "1.0.0".into()
}
fn all_checks() -> Vec<Check> {
    Check::ALL.to_vec()
}
fn default_max_calls() -> usize {
    20
}

impl Default for SurfaceManifest {
    fn default() -> Self {
        Self {
            id: "default".into(),
            version: default_version(),
            description: "All deterministic checks; no judge.".into(),
            checks: all_checks(),
            thresholds: Thresholds::default(),
            rules: vec![],
            judge: JudgePolicy::default(),
        }
    }
}

impl SurfaceManifest {
    pub fn from_yaml(src: &str) -> Result<Self, String> {
        let m: Self = serde_yaml::from_str(src).map_err(|e| e.to_string())?;
        m.check()?;
        Ok(m)
    }

    pub fn check(&self) -> Result<(), String> {
        let t = &self.thresholds;
        for (name, v) in [("support", t.support), ("related", t.related), ("qualification", t.qualification), ("overclaim", t.overclaim)] {
            if !(0.0..=1.0).contains(&v) {
                return Err(format!("thresholds.{name} must be within [0, 1]"));
            }
        }
        if t.related > t.support || t.qualification_floor > t.qualification {
            return Err("lower thresholds must not exceed upper ones".into());
        }
        for r in &self.rules {
            if r.forbid.is_empty() && r.require_any.is_empty() {
                return Err(format!("rule '{}' must forbid or require something", r.id));
            }
        }
        if self.judge.discretion == JudgeDiscretion::None && !self.judge.may_answer.is_empty() {
            return Err("judge.may_answer requires a discretion other than none".into());
        }
        Ok(())
    }

    pub fn runs(&self, c: Check) -> bool {
        self.checks.contains(&c)
    }

    pub fn judge_may(&self, k: ResidualKind) -> bool {
        self.judge.discretion != JudgeDiscretion::None && self.judge.may_answer.contains(&k)
    }
}
