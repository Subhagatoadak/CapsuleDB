//! Maps answer claims to the capsules that support them: an explicit
//! `kc://` citation always counts; otherwise, a lexical (Jaccard word
//! overlap) similarity between the claim and a capsule's statement. Two
//! overlap bands are tracked — `strong` (confident support) and `weak`
//! (possible but ambiguous support) — since the gap between them is
//! exactly the "residual semantic question" a bounded judge exists for
//! (see `judge.rs`).

use crate::claims::AnswerClaim;
use crate::error::EvaluatorError;
use capsule_compiler::classify::detect_dependency_refs;
use capsule_registry::{CapsuleRecord, Registry};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportStrength {
    Strong,
    Weak,
}

#[derive(Debug, Clone)]
pub struct Support {
    pub record: CapsuleRecord,
    pub strength: SupportStrength,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct ClaimMapping {
    pub claim_index: usize,
    pub supporting: Vec<Support>,
}

impl ClaimMapping {
    pub fn strong_support(&self) -> impl Iterator<Item = &Support> {
        self.supporting.iter().filter(|s| s.strength == SupportStrength::Strong)
    }

    pub fn is_unsupported(&self) -> bool {
        self.supporting.is_empty()
    }

    pub fn is_ambiguous(&self) -> bool {
        !self.supporting.is_empty() && self.strong_support().next().is_none()
    }
}

pub fn map_claims_to_capsules(
    claims: &[AnswerClaim],
    registry: &Registry,
    strong_threshold: f64,
    weak_threshold: f64,
) -> Result<Vec<ClaimMapping>, EvaluatorError> {
    let mut candidates = Vec::new();
    for logical_id in registry.list_logical_ids()? {
        if let Some(record) = registry.latest_published(&logical_id, None)? {
            candidates.push(record);
        }
    }

    let mut mappings = Vec::with_capacity(claims.len());
    for (claim_index, claim) in claims.iter().enumerate() {
        let claim_words = word_set(&claim.text);
        let explicit_refs = detect_dependency_refs(&claim.text);
        let mut supporting = Vec::new();

        for record in &candidates {
            let explicitly_cited = explicit_refs.iter().any(|r| r.starts_with(&record.logical_id));
            let score = jaccard(&claim_words, &word_set(&record.capsule.statement));

            let strength = if explicitly_cited || score >= strong_threshold {
                Some(SupportStrength::Strong)
            } else if score >= weak_threshold {
                Some(SupportStrength::Weak)
            } else {
                None
            };

            if let Some(strength) = strength {
                supporting.push(Support { record: record.clone(), strength, score: if explicitly_cited { 1.0 } else { score } });
            }
        }

        mappings.push(ClaimMapping { claim_index, supporting });
    }

    Ok(mappings)
}

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "to", "of", "in", "on", "for", "and", "or", "but", "with",
    "that", "this", "it", "as", "by", "at", "from", "not",
];

fn word_set(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOPWORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    intersection / union
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
    fn maps_by_lexical_overlap() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1", "logical_id": "kc://policy/retention/rule", "version": "1.0.0",
              "capsule_type": "rule", "statement": "Customer data must be retained for ninety days.",
              "epistemic_status": "policy_defined", "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
              "lifecycle_state": "draft", "governance": {"owner": "alice", "domain": "policy"}
            }"#,
        );

        let claims = vec![crate::claims::AnswerClaim { text: "Customer data is retained for ninety days.".to_string(), start: 0, end: 10 }];
        let mappings = map_claims_to_capsules(&claims, &registry, 0.4, 0.15).unwrap();
        assert_eq!(mappings[0].supporting.len(), 1);
        assert_eq!(mappings[0].supporting[0].strength, SupportStrength::Strong);
    }

    #[test]
    fn explicit_citation_always_maps_strongly() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1", "logical_id": "kc://policy/retention/rule", "version": "1.0.0",
              "capsule_type": "rule", "statement": "Totally different wording that shares nothing.",
              "epistemic_status": "policy_defined", "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
              "lifecycle_state": "draft", "governance": {"owner": "alice", "domain": "policy"}
            }"#,
        );

        let claims = vec![crate::claims::AnswerClaim {
            text: "See kc://policy/retention/rule@1.0.0 for details.".to_string(),
            start: 0,
            end: 10,
        }];
        let mappings = map_claims_to_capsules(&claims, &registry, 0.9, 0.5).unwrap();
        assert_eq!(mappings[0].supporting.len(), 1);
        assert_eq!(mappings[0].supporting[0].strength, SupportStrength::Strong);
    }

    #[test]
    fn unrelated_claim_is_unsupported() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1", "logical_id": "kc://policy/retention/rule", "version": "1.0.0",
              "capsule_type": "rule", "statement": "Customer data must be retained for ninety days.",
              "epistemic_status": "policy_defined", "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
              "lifecycle_state": "draft", "governance": {"owner": "alice", "domain": "policy"}
            }"#,
        );

        let claims = vec![crate::claims::AnswerClaim { text: "The weather today is sunny and warm.".to_string(), start: 0, end: 10 }];
        let mappings = map_claims_to_capsules(&claims, &registry, 0.4, 0.15).unwrap();
        assert!(mappings[0].is_unsupported());
    }
}
