//! Deterministic evaluation rules: every detector here runs before (and
//! instead of, in this MVP with no LLM step) any judge — the "answer
//! surface" a bounded judge would otherwise have to discover, interpret,
//! and score all at once.

use crate::claims::AnswerClaim;
use crate::mapping::ClaimMapping;
use capsule_compiler::detect_inflation;
use capsule_registry::CapsuleRecord;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingKind {
    UnsupportedClaim,
    ApplicabilityExpansion,
    InferenceToFact,
    AssociationToCausation,
    ConfidenceCeilingViolation,
    MissingMandatoryQualification,
    ContradictionSuppression,
    EvidenceMultiplication,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub claim_index: Option<usize>,
    pub capsule_logical_id: Option<String>,
    pub message: String,
}

const UNIVERSAL_MARKERS: &[&str] = &["always", "in all cases", "everywhere", "for everyone", "under all circumstances", "without exception"];
const ABSOLUTE_CERTAINTY_MARKERS: &[&str] = &["definitely", "certainly", "guaranteed", "proven fact", "without doubt", "100%", "always true"];
const CONFIDENCE_CEILING_ALARM_THRESHOLD: f64 = 0.7;
const DISCLOSURE_MARKERS: &[&str] = &["however", "disputed", "unresolved", "contested", "note:", "but see", "contradict"];

pub fn detect_unsupported_claim(mapping: &ClaimMapping) -> Option<Finding> {
    if mapping.is_unsupported() {
        Some(Finding {
            kind: FindingKind::UnsupportedClaim,
            claim_index: Some(mapping.claim_index),
            capsule_logical_id: None,
            message: "no registered capsule supports this claim, even weakly".to_string(),
        })
    } else {
        None
    }
}

pub fn detect_applicability_expansion(claim: &AnswerClaim, mapping: &ClaimMapping) -> Vec<Finding> {
    let lower = claim.text.to_lowercase();
    if !UNIVERSAL_MARKERS.iter().any(|m| lower.contains(m)) {
        return vec![];
    }
    mapping
        .strong_support()
        .filter(|s| {
            s.record
                .capsule
                .applicability
                .as_ref()
                .map(|a| !a.conditions.is_empty() || !a.exclusions.is_empty() || !a.jurisdictions.is_empty())
                .unwrap_or(false)
        })
        .map(|s| Finding {
            kind: FindingKind::ApplicabilityExpansion,
            claim_index: Some(mapping.claim_index),
            capsule_logical_id: Some(s.record.logical_id.clone()),
            message: format!(
                "claim uses universal language ('always'/'everywhere'/...) but {} has stated applicability limits",
                s.record.logical_id
            ),
        })
        .collect()
}

/// Reuses `capsule-compiler`'s confidence-laundering pattern detector,
/// comparing the capsule's original statement against the claim's
/// phrasing, and classifies each pattern into the roadmap's
/// inference-to-fact vs. association-to-causation buckets.
pub fn detect_confidence_laundering(claim: &AnswerClaim, mapping: &ClaimMapping) -> Vec<Finding> {
    mapping
        .strong_support()
        .flat_map(|support| {
            detect_inflation(&support.record.capsule.statement, &claim.text).into_iter().map(move |finding| {
                let kind = if finding.pattern == "associated with -> causes" { FindingKind::AssociationToCausation } else { FindingKind::InferenceToFact };
                Finding {
                    kind,
                    claim_index: None, // filled by caller
                    capsule_logical_id: Some(support.record.logical_id.clone()),
                    message: format!("claim escalates '{}' to '{}' relative to {}", finding.weak_phrase, finding.inflated_phrase, support.record.logical_id),
                }
            })
        })
        .map(|mut f| {
            f.claim_index = Some(mapping.claim_index);
            f
        })
        .collect()
}

pub fn detect_confidence_ceiling_violation(claim: &AnswerClaim, mapping: &ClaimMapping) -> Vec<Finding> {
    let lower = claim.text.to_lowercase();
    if !ABSOLUTE_CERTAINTY_MARKERS.iter().any(|m| lower.contains(m)) {
        return vec![];
    }
    mapping
        .strong_support()
        .filter(|s| s.record.capsule.confidence.ceiling < CONFIDENCE_CEILING_ALARM_THRESHOLD)
        .map(|s| Finding {
            kind: FindingKind::ConfidenceCeilingViolation,
            claim_index: Some(mapping.claim_index),
            capsule_logical_id: Some(s.record.logical_id.clone()),
            message: format!(
                "claim asserts absolute certainty but {} has confidence ceiling {:.2}",
                s.record.logical_id, s.record.capsule.confidence.ceiling
            ),
        })
        .collect()
}

/// Checks each supporting capsule's `evaluation_contract.mandatory_qualifications`
/// against the **whole answer**, not just the citing sentence — a
/// qualification stated in an adjacent sentence still counts as
/// disclosed.
pub fn detect_missing_mandatory_qualifications(answer: &str, mapping: &ClaimMapping) -> Vec<Finding> {
    let answer_lower = answer.to_lowercase();
    mapping
        .strong_support()
        .flat_map(|s| {
            s.record.capsule.evaluation_contract.iter().flat_map(|contract| &contract.mandatory_qualifications).filter_map(|q| {
                let key_phrase = q.to_lowercase();
                if answer_lower.contains(&key_phrase) {
                    None
                } else {
                    Some(Finding {
                        kind: FindingKind::MissingMandatoryQualification,
                        claim_index: Some(mapping.claim_index),
                        capsule_logical_id: Some(s.record.logical_id.clone()),
                        message: format!("answer never states {}'s mandatory qualification: \"{}\"", s.record.logical_id, q),
                    })
                }
            })
        })
        .collect()
}

pub fn detect_contradiction_suppression(answer: &str, mapping: &ClaimMapping) -> Vec<Finding> {
    let answer_lower = answer.to_lowercase();
    let generic_disclosure = DISCLOSURE_MARKERS.iter().any(|m| answer_lower.contains(m));

    mapping
        .strong_support()
        .flat_map(|s| {
            let answer_lower = &answer_lower;
            s.record
                .capsule
                .contradictions
                .iter()
                .filter(|c| c.resolution_status == capsule_core::ResolutionStatus::Unresolved)
                .filter_map(move |c| {
                    let disclosed = generic_disclosure || answer_lower.contains(&c.capsule_id.to_lowercase());
                    if disclosed {
                        None
                    } else {
                        Some(Finding {
                            kind: FindingKind::ContradictionSuppression,
                            claim_index: Some(mapping.claim_index),
                            capsule_logical_id: Some(s.record.logical_id.clone()),
                            message: format!("{} has an unresolved contradiction the answer never discloses: {}", s.record.logical_id, c.description),
                        })
                    }
                })
        })
        .collect()
}

/// Groups every capsule cited (strongly) across the whole answer by
/// evidence `source_id`; two or more distinct capsules citing the same
/// source are not independent evidence for whatever the answer concludes
/// from them together.
pub fn detect_evidence_multiplication(mappings: &[ClaimMapping]) -> Vec<Finding> {
    let mut by_source: HashMap<String, HashSet<String>> = HashMap::new();
    let mut records_by_id: HashMap<String, &CapsuleRecord> = HashMap::new();

    for mapping in mappings {
        for support in mapping.strong_support() {
            records_by_id.insert(support.record.logical_id.clone(), &support.record);
            for evidence in &support.record.capsule.evidence {
                by_source.entry(evidence.source_id.clone()).or_default().insert(support.record.logical_id.clone());
            }
        }
    }

    let mut findings: Vec<Finding> = by_source
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(source, ids)| {
            let mut sorted: Vec<String> = ids.into_iter().collect();
            sorted.sort();
            Finding {
                kind: FindingKind::EvidenceMultiplication,
                claim_index: None,
                capsule_logical_id: None,
                message: format!("{} capsules cite the same evidence source '{}', so they are not independent evidence: {}", sorted.len(), source, sorted.join(", ")),
            }
        })
        .collect();
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{Support, SupportStrength};
    use capsule_core::*;

    fn record(logical_id: &str, statement: &str) -> CapsuleRecord {
        CapsuleRecord {
            logical_id: logical_id.to_string(),
            version: "1.0.0".to_string(),
            content_hash: "sha256:x".to_string(),
            lifecycle_state: LifecycleState::Published,
            recorded_at: "now".to_string(),
            capsule: Capsule {
                schema_version: SCHEMA_VERSION.to_string(),
                logical_id: logical_id.to_string(),
                version: "1.0.0".to_string(),
                content_hash: None,
                capsule_type: CapsuleType::Fact,
                statement: statement.to_string(),
                epistemic_status: EpistemicStatus::Estimated,
                confidence: Confidence { basis: ConfidenceBasis::StatisticalInference, ceiling: 0.5, rationale: None },
                lifecycle_state: LifecycleState::Published,
                evidence: vec![],
                applicability: None,
                contradictions: vec![],
                dependencies: vec![],
                governance: Governance { owner: "a".to_string(), domain: "d".to_string(), permissions: vec![] },
                evaluation_contract: None,
            },
        }
    }

    #[test]
    fn detects_unsupported_claims() {
        let mapping = ClaimMapping { claim_index: 0, supporting: vec![] };
        assert!(detect_unsupported_claim(&mapping).is_some());
    }

    #[test]
    fn detects_estimated_to_observed_as_inference_to_fact() {
        let claim = AnswerClaim { text: "The lift was observed at 6%.".to_string(), start: 0, end: 1 };
        let capsule = record("kc://a/b/c", "The lift was estimated at 6%.");
        let mapping = ClaimMapping { claim_index: 0, supporting: vec![Support { record: capsule, strength: SupportStrength::Strong, score: 1.0 }] };
        let findings = detect_confidence_laundering(&claim, &mapping);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::InferenceToFact);
    }

    #[test]
    fn detects_association_to_causation() {
        let claim = AnswerClaim { text: "Smoking causes the disease.".to_string(), start: 0, end: 1 };
        let capsule = record("kc://a/b/c", "Smoking is associated with the disease.");
        let mapping = ClaimMapping { claim_index: 0, supporting: vec![Support { record: capsule, strength: SupportStrength::Strong, score: 1.0 }] };
        let findings = detect_confidence_laundering(&claim, &mapping);
        assert_eq!(findings[0].kind, FindingKind::AssociationToCausation);
    }

    #[test]
    fn detects_confidence_ceiling_violation() {
        let claim = AnswerClaim { text: "This is definitely true.".to_string(), start: 0, end: 1 };
        let capsule = record("kc://a/b/c", "x");
        let mapping = ClaimMapping { claim_index: 0, supporting: vec![Support { record: capsule, strength: SupportStrength::Strong, score: 1.0 }] };
        let findings = detect_confidence_ceiling_violation(&claim, &mapping);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn missing_qualification_is_detected_across_whole_answer() {
        let mut capsule_record = record("kc://a/b/c", "x");
        capsule_record.capsule.evaluation_contract =
            Some(EvaluationContract { mandatory_qualifications: vec!["unless a legal hold applies".to_string()], ..Default::default() });
        let mapping = ClaimMapping { claim_index: 0, supporting: vec![Support { record: capsule_record, strength: SupportStrength::Strong, score: 1.0 }] };

        let answer_missing = "Data is retained for 90 days.";
        assert_eq!(detect_missing_mandatory_qualifications(answer_missing, &mapping).len(), 1);

        let answer_present = "Data is retained for 90 days. This is unless a legal hold applies, in which case longer.";
        assert!(detect_missing_mandatory_qualifications(answer_present, &mapping).is_empty());
    }

    #[test]
    fn evidence_multiplication_flags_shared_sources_across_claims() {
        let mut a = record("kc://a/b/c", "x");
        a.capsule.evidence = vec![Evidence { source_id: "shared-report".to_string(), span: None, excerpt: None, authority: EvidenceAuthority::Primary, retrieved_at: None }];
        let mut b = record("kc://a/b/d", "y");
        b.capsule.evidence = vec![Evidence { source_id: "shared-report".to_string(), span: None, excerpt: None, authority: EvidenceAuthority::Primary, retrieved_at: None }];

        let mappings = vec![
            ClaimMapping { claim_index: 0, supporting: vec![Support { record: a, strength: SupportStrength::Strong, score: 1.0 }] },
            ClaimMapping { claim_index: 1, supporting: vec![Support { record: b, strength: SupportStrength::Strong, score: 1.0 }] },
        ];
        let findings = detect_evidence_multiplication(&mappings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::EvidenceMultiplication);
    }
}
