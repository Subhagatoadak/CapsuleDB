//! `capsule-evaluator`: evaluates an answer against the capsules that
//! back it (Phase 6), per the top-level README's target architecture:
//!
//! ```text
//! Answer -> Claim decomposition -> Capsule resolution -> Deterministic validation
//!        -> Epistemic validation -> Bounded LLM judge -> Evaluation report
//! ```
//!
//! Every detector here is deterministic and runs first; a [`judge::Judge`]
//! is only ever consulted for claims whose capsule support is
//! genuinely ambiguous (weak-but-not-strong lexical overlap), and this
//! crate ships no real judge — [`judge::NullJudge`] always abstains. See
//! `judge.rs` for why that boundary, not the judge itself, is the point.

pub mod claims;
pub mod detectors;
pub mod error;
pub mod judge;
pub mod mapping;
pub mod report;

pub use claims::{decompose_claims, AnswerClaim};
pub use detectors::{Finding, FindingKind};
pub use error::EvaluatorError;
pub use judge::{DiscretionLevel, Judge, JudgeVerdict, NullJudge, ResidualQuestion};
pub use mapping::{map_claims_to_capsules, ClaimMapping, Support, SupportStrength};
pub use report::{EvaluationReport, EvaluationSurfaceManifest, Scores};

use capsule_registry::Registry;

#[derive(Debug, Clone)]
pub struct EvaluateOptions {
    pub discretion: DiscretionLevel,
    pub strong_support_threshold: f64,
}

impl Default for EvaluateOptions {
    fn default() -> Self {
        Self { discretion: DiscretionLevel::Low, strong_support_threshold: 0.4 }
    }
}

/// Runs the full pipeline: decompose `answer` into claims, map each to
/// supporting capsules in `registry`, run every deterministic detector,
/// consult `judge` for any claim left ambiguous, and return the report.
pub fn evaluate(answer: &str, registry: &Registry, judge: &dyn Judge, options: &EvaluateOptions) -> Result<EvaluationReport, EvaluatorError> {
    let answer_claims = decompose_claims(answer);
    let weak_threshold = options.discretion.weak_threshold(options.strong_support_threshold);
    let mappings = map_claims_to_capsules(&answer_claims, registry, options.strong_support_threshold, weak_threshold)?;

    let mut findings = Vec::new();
    for (claim, mapping) in answer_claims.iter().zip(mappings.iter()) {
        if let Some(f) = detectors::detect_unsupported_claim(mapping) {
            findings.push(f);
        }
        findings.extend(detectors::detect_applicability_expansion(claim, mapping));
        findings.extend(detectors::detect_confidence_laundering(claim, mapping));
        findings.extend(detectors::detect_confidence_ceiling_violation(claim, mapping));
        findings.extend(detectors::detect_missing_mandatory_qualifications(answer, mapping));
        findings.extend(detectors::detect_contradiction_suppression(answer, mapping));
    }
    findings.extend(detectors::detect_evidence_multiplication(&mappings));

    let mut judge_verdicts = Vec::new();
    for (claim, mapping) in answer_claims.iter().zip(mappings.iter()) {
        if mapping.is_ambiguous() {
            let question = ResidualQuestion {
                claim_index: mapping.claim_index,
                claim_text: claim.text.clone(),
                candidate_capsules: mapping.supporting.iter().map(|s| s.record.logical_id.clone()).collect(),
            };
            judge_verdicts.push((mapping.claim_index, judge.judge(&question)));
        }
    }

    Ok(report::build_report(&mappings, findings, judge_verdicts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, LifecycleState};

    fn publish(registry: &mut Registry, json: &str) {
        let capsule = parse(json).unwrap();
        let record = registry.register(&capsule, "alice").unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::InReview, "alice", None).unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::Approved, "alice", None).unwrap();
        registry.publish(&record.logical_id, &record.version, "alice").unwrap();
    }

    #[test]
    fn end_to_end_flags_inflation_and_missing_qualification() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1",
              "logical_id": "kc://policy/retention/rule",
              "version": "1.0.0",
              "capsule_type": "rule",
              "statement": "Data retention is estimated at ninety days unless a legal hold applies.",
              "epistemic_status": "estimated",
              "confidence": {"basis": "statistical_inference", "ceiling": 0.5},
              "lifecycle_state": "draft",
              "evaluation_contract": {"mandatory_qualifications": ["unless a legal hold applies"]},
              "governance": {"owner": "alice", "domain": "policy"}
            }"#,
        );

        let answer = "Data retention is observed at ninety days.";
        let report = evaluate(answer, &registry, &NullJudge, &EvaluateOptions::default()).unwrap();

        assert!(report.findings.iter().any(|f| f.kind == FindingKind::InferenceToFact));
        assert!(report.findings.iter().any(|f| f.kind == FindingKind::MissingMandatoryQualification));
        assert_eq!(report.surface.total_claims, 1);
        assert!(report.scores.confidence_laundering_risk > 0.0);
    }

    #[test]
    fn end_to_end_clean_answer_has_no_findings() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1",
              "logical_id": "kc://policy/retention/rule",
              "version": "1.0.0",
              "capsule_type": "rule",
              "statement": "Data must be retained for ninety days.",
              "epistemic_status": "policy_defined",
              "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
              "lifecycle_state": "draft",
              "governance": {"owner": "alice", "domain": "policy"}
            }"#,
        );

        let answer = "Data must be retained for ninety days.";
        let report = evaluate(answer, &registry, &NullJudge, &EvaluateOptions::default()).unwrap();
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.scores.epistemic_fidelity, 1.0);
    }

    #[test]
    fn unsupported_claim_reduces_epistemic_fidelity_and_surface_reduction_still_counts_it() {
        let registry = Registry::open_in_memory().unwrap();
        let answer = "The moon is made of green cheese.";
        let report = evaluate(answer, &registry, &NullJudge, &EvaluateOptions::default()).unwrap();
        assert_eq!(report.surface.claims_unsupported, 1);
        // an unsupported claim is still a deterministic verdict (not a judge referral)
        assert_eq!(report.scores.evaluation_surface_reduction, 1.0);
    }
}
