//! The compile pipeline: source -> candidate draft capsules, with
//! duplicate-evidence detection and deterministic pre-validation.
//!
//! Every candidate is deliberately conservative: `evidence[].authority`
//! is always `unverified` (we don't know provenance strength from text
//! alone) and `confidence.basis` is always `heuristic` with a capped
//! `ceiling` — a human upgrading the evidence and confidence is expected
//! before a candidate is registered. Concretely, a candidate whose
//! `epistemic_status` classifies as `observed` or `expert_judgement` will
//! *fail* `capsule_core::validate`'s mandatory-evidence rule until that
//! happens — the compiler and the core validator deliberately disagree
//! until a human resolves it.

use crate::classify::{classify_epistemic_status, detect_dependency_refs, extract_applicability_conditions, extract_exclusions, extract_qualifications};
use crate::lineage::Lineage;
use crate::segmentation::extract_claims;
use crate::source::Source;
use capsule_core::{
    validate, Applicability, Capsule, CapsuleType, Confidence, ConfidenceBasis, EpistemicStatus, EvaluationContract, Evidence,
    EvidenceAuthority, Governance, LifecycleState, ValidationIssue, SCHEMA_VERSION,
};
use std::collections::HashMap;

pub struct CompileOptions {
    pub domain: String,
    pub topic: String,
    pub owner: String,
}

pub struct CandidateCapsule {
    pub capsule: Capsule,
    pub lineage: Lineage,
    pub qualifications: Vec<String>,
    pub exclusions: Vec<String>,
    pub applicability_conditions: Vec<String>,
    /// Bare `kc://` mentions found in the claim text; not auto-wired into
    /// `capsule.dependencies` (see `classify::detect_dependency_refs`).
    pub referenced_capsules: Vec<String>,
    pub validation_issues: Vec<ValidationIssue>,
}

pub struct DuplicateFinding {
    pub text: String,
    pub first_candidate_index: usize,
    pub span: (usize, usize),
}

pub struct CompilationReport {
    pub source_id: String,
    pub candidates: Vec<CandidateCapsule>,
    pub duplicates: Vec<DuplicateFinding>,
}

pub fn compile(source: &Source, options: &CompileOptions) -> CompilationReport {
    let mut seen_fingerprints: HashMap<String, usize> = HashMap::new();
    let mut candidates = Vec::new();
    let mut duplicates = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for claim in extract_claims(source) {
        let fingerprint = capsule_core::source_fingerprint(claim.text.as_bytes());
        if let Some(&first_index) = seen_fingerprints.get(&fingerprint) {
            duplicates.push(DuplicateFinding { text: claim.text.clone(), first_candidate_index: first_index, span: (claim.start, claim.end) });
            continue;
        }

        let index = candidates.len();
        seen_fingerprints.insert(fingerprint, index);

        let epistemic_status = classify_epistemic_status(&claim.text);
        let capsule_type = if epistemic_status == EpistemicStatus::PolicyDefined { CapsuleType::Rule } else { CapsuleType::Fact };
        let qualifications = extract_qualifications(&claim.text);
        let exclusions = extract_exclusions(&claim.text);
        let conditions = extract_applicability_conditions(&claim.text);
        let referenced_capsules = detect_dependency_refs(&claim.text);

        let slug = slugify(&claim.text, 6);
        let logical_id = format!("kc://{}/{}/{}-{}", options.domain, options.topic, slug, index + 1);

        let applicability = if conditions.is_empty() && exclusions.is_empty() {
            None
        } else {
            Some(Applicability { conditions: conditions.clone(), exclusions: exclusions.clone(), ..Default::default() })
        };
        // Qualifications ("unless X applies") aren't an applicability field
        // in the KCP 0.1 schema; they map to the evaluation contract's
        // mandatory_qualifications, since that's exactly what it's for —
        // language any answer citing this capsule must carry.
        let evaluation_contract = if qualifications.is_empty() {
            None
        } else {
            Some(EvaluationContract { mandatory_qualifications: qualifications.clone(), ..Default::default() })
        };

        let capsule = Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            logical_id,
            version: "0.1.0".to_string(),
            content_hash: None,
            capsule_type,
            statement: claim.text.clone(),
            epistemic_status,
            confidence: Confidence {
                basis: ConfidenceBasis::Heuristic,
                ceiling: 0.4,
                rationale: Some("auto-extracted by capsule-compiler; pending human review".to_string()),
            },
            lifecycle_state: LifecycleState::Draft,
            evidence: vec![Evidence {
                source_id: source.id.clone(),
                span: Some(format!("block {} offset {}-{}", claim.block_index, claim.start, claim.end)),
                excerpt: Some(claim.text.clone()),
                authority: EvidenceAuthority::Unverified,
                retrieved_at: Some(now.clone()),
            }],
            applicability,
            contradictions: vec![],
            dependencies: vec![],
            governance: Governance { owner: options.owner.clone(), domain: options.domain.clone(), permissions: vec![] },
            evaluation_contract,
        };

        let validation_issues = validate(&capsule);

        candidates.push(CandidateCapsule {
            capsule,
            lineage: Lineage {
                source_id: source.id.clone(),
                block_index: claim.block_index,
                span: (claim.start, claim.end),
                rules_applied: vec!["sentence-segmentation".to_string(), "lexical-epistemic-classifier".to_string()],
            },
            qualifications,
            exclusions,
            applicability_conditions: conditions,
            referenced_capsules,
            validation_issues,
        });
    }

    CompilationReport { source_id: source.id.clone(), candidates, duplicates }
}

fn slugify(text: &str, max_words: usize) -> String {
    let slug: Vec<String> = text
        .split_whitespace()
        .take(max_words)
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    if slug.is_empty() {
        "claim".to_string()
    } else {
        slug.join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ingest_text;
    use capsule_core::Severity;

    fn options() -> CompileOptions {
        CompileOptions { domain: "policy".to_string(), topic: "retention".to_string(), owner: "alice".to_string() }
    }

    #[test]
    fn compiles_one_candidate_per_sentence() {
        let source = ingest_text(b"Data must be retained for 90 days. Except for legal holds.").unwrap();
        let report = compile(&source, &options());
        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.candidates[0].capsule.statement, "Data must be retained for 90 days.");
        assert_eq!(report.candidates[0].capsule.epistemic_status, EpistemicStatus::PolicyDefined);
        assert_eq!(report.candidates[0].capsule.capsule_type, CapsuleType::Rule);
    }

    #[test]
    fn candidate_logical_ids_are_well_formed_and_unique() {
        let source = ingest_text(b"First rule applies. Second rule applies.").unwrap();
        let report = compile(&source, &options());
        let ids: Vec<&str> = report.candidates.iter().map(|c| c.capsule.logical_id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        for id in ids {
            assert!(id.starts_with("kc://policy/retention/"));
        }
    }

    #[test]
    fn exact_duplicate_sentences_are_flagged_not_duplicated() {
        let source = ingest_text(b"Retention is 90 days. Some other clause. Retention is 90 days.").unwrap();
        let report = compile(&source, &options());
        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].text, "Retention is 90 days.");
    }

    #[test]
    fn qualifications_populate_the_evaluation_contract() {
        let source = ingest_text(b"Retention is 90 days unless a legal hold applies.").unwrap();
        let report = compile(&source, &options());
        let candidate = &report.candidates[0];
        assert_eq!(candidate.qualifications, vec!["unless a legal hold applies"]);
        let contract = candidate.capsule.evaluation_contract.as_ref().expect("evaluation_contract should be set");
        assert_eq!(contract.mandatory_qualifications, vec!["unless a legal hold applies"]);
    }

    #[test]
    fn exclusions_populate_applicability() {
        let source = ingest_text(b"Applies to all staff except contractors.").unwrap();
        let report = compile(&source, &options());
        let candidate = &report.candidates[0];
        let applicability = candidate.capsule.applicability.as_ref().expect("applicability should be set");
        assert_eq!(applicability.exclusions, vec!["except contractors"]);
    }

    #[test]
    fn observed_claim_without_authoritative_evidence_fails_deterministic_validation() {
        let source = ingest_text(b"We observed a 12% increase in signups.").unwrap();
        let report = compile(&source, &options());
        let candidate = &report.candidates[0];
        assert_eq!(candidate.capsule.epistemic_status, EpistemicStatus::Observed);
        assert!(candidate.validation_issues.iter().any(|i| i.code == "mandatory_evidence" && i.severity == Severity::Error));
    }

    #[test]
    fn policy_defined_claim_with_unverified_evidence_still_passes() {
        let source = ingest_text(b"Data must be retained for 90 days.").unwrap();
        let report = compile(&source, &options());
        let candidate = &report.candidates[0];
        assert!(!candidate.validation_issues.iter().any(|i| i.severity == Severity::Error), "{:?}", candidate.validation_issues);
    }

    #[test]
    fn referenced_capsules_are_surfaced_but_not_wired_as_dependencies() {
        let source = ingest_text(b"This refines kc://policy/retention/base@1.0.0 for EU customers.").unwrap();
        let report = compile(&source, &options());
        let candidate = &report.candidates[0];
        assert_eq!(candidate.referenced_capsules, vec!["kc://policy/retention/base@1.0.0"]);
        assert!(candidate.capsule.dependencies.is_empty());
    }
}
