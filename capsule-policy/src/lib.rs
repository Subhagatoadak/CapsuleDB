//! Policy-as-code (Phase 10). `capsule_core::validate` enforces what KCP
//! 0.1 itself requires — every capsule, in every deployment. A
//! [`PolicyEngine`] enforces what *this deployment* additionally
//! requires: its own domain allowlist, a confidence ceiling tighter than
//! the protocol's per-basis hard cap, an owner-identity pattern, banned
//! statement phrasing. A capsule can be protocol-valid and still violate
//! policy — the engine is meant to run alongside `validate`, not replace
//! it, typically gating `capsule registry register`/`publish`.
//!
//! Policy is data, not code: a deployment writes a JSON file of
//! [`PolicyRule`]s and points `capsule-cli`/`capsule-api` at it, rather
//! than forking this crate to add a check.

use capsule_core::{Capsule, CapsuleType, ConfidenceBasis};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyRule {
    /// `governance.domain` must be one of `domains`.
    AllowedDomains { domains: Vec<String> },
    /// A capsule whose `confidence.basis` is `basis` must not declare a
    /// ceiling above `max` — a per-deployment tightening of the
    /// protocol's own (looser) per-basis hard cap.
    MaxConfidenceCeiling { basis: ConfidenceBasis, max: f64 },
    /// `governance.owner` must match this regex (e.g. requiring an
    /// internal email domain or a `team:` prefix).
    OwnerPattern { pattern: String },
    /// A capsule of `capsule_type` must cite at least `min_evidence`
    /// evidence entries — stricter than the protocol's blanket "at least
    /// one" for `fact`/`observed` (spec's `mandatory_evidence` check).
    MinEvidenceForType { capsule_type: CapsuleType, min_evidence: usize },
    /// `statement` must not contain `phrase` (case-insensitive) — for
    /// banning specific overclaiming language beyond the built-in
    /// inflation detectors (`capsule-compiler::detect_inflation`).
    ForbiddenStatementPhrase { phrase: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyViolation {
    pub rule: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("reading policy file: {0}")]
    Io(#[from] std::io::Error),
    #[error("parsing policy file: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid regex in owner_pattern rule '{0}': {1}")]
    Regex(String, regex::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyDocument {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    pub fn from_rules(rules: Vec<PolicyRule>) -> Self {
        Self { rules }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let text = std::fs::read_to_string(path)?;
        let doc: PolicyDocument = serde_json::from_str(&text)?;
        Ok(Self::from_rules(doc.rules))
    }

    /// Every violation this capsule commits against the loaded rule set.
    /// An empty result means the capsule is policy-clean; it says nothing
    /// about protocol validity — callers should also run
    /// `capsule_core::validate`.
    pub fn evaluate(&self, capsule: &Capsule) -> Result<Vec<PolicyViolation>, PolicyError> {
        let mut violations = Vec::new();
        for rule in &self.rules {
            match rule {
                PolicyRule::AllowedDomains { domains } => {
                    if !domains.iter().any(|d| d == &capsule.governance.domain) {
                        violations.push(PolicyViolation {
                            rule: "allowed_domains".to_string(),
                            message: format!("governance.domain '{}' is not in the allowed set {domains:?}", capsule.governance.domain),
                        });
                    }
                }
                PolicyRule::MaxConfidenceCeiling { basis, max } => {
                    if capsule.confidence.basis == *basis && capsule.confidence.ceiling > *max {
                        violations.push(PolicyViolation {
                            rule: "max_confidence_ceiling".to_string(),
                            message: format!(
                                "confidence.ceiling {:.2} exceeds this deployment's max {:.2} for basis {basis:?}",
                                capsule.confidence.ceiling, max
                            ),
                        });
                    }
                }
                PolicyRule::OwnerPattern { pattern } => {
                    let re = Regex::new(pattern).map_err(|e| PolicyError::Regex(pattern.clone(), e))?;
                    if !re.is_match(&capsule.governance.owner) {
                        violations.push(PolicyViolation {
                            rule: "owner_pattern".to_string(),
                            message: format!("governance.owner '{}' does not match required pattern '{pattern}'", capsule.governance.owner),
                        });
                    }
                }
                PolicyRule::MinEvidenceForType { capsule_type, min_evidence } => {
                    if capsule.capsule_type == *capsule_type && capsule.evidence.len() < *min_evidence {
                        violations.push(PolicyViolation {
                            rule: "min_evidence_for_type".to_string(),
                            message: format!(
                                "capsule_type {capsule_type:?} requires at least {min_evidence} evidence entries under this deployment's policy, found {}",
                                capsule.evidence.len()
                            ),
                        });
                    }
                }
                PolicyRule::ForbiddenStatementPhrase { phrase } => {
                    if capsule.statement.to_lowercase().contains(&phrase.to_lowercase()) {
                        violations.push(PolicyViolation {
                            rule: "forbidden_statement_phrase".to_string(),
                            message: format!("statement contains the forbidden phrase '{phrase}'"),
                        });
                    }
                }
            }
        }
        Ok(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::parse;

    fn capsule(json: &str) -> Capsule {
        parse(json).unwrap()
    }

    fn base_json(domain: &str, owner: &str, statement: &str) -> String {
        format!(
            r#"{{
              "schema_version": "kcp/0.1", "logical_id": "kc://{domain}/x/y", "version": "1.0.0",
              "capsule_type": "rule", "statement": "{statement}", "epistemic_status": "policy_defined",
              "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}}, "lifecycle_state": "draft",
              "governance": {{"owner": "{owner}", "domain": "{domain}"}}
            }}"#
        )
    }

    #[test]
    fn allowed_domains_rejects_a_domain_outside_the_list() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::AllowedDomains { domains: vec!["policy".to_string()] }]);
        let c = capsule(&base_json("marketing", "alice", "x"));
        let violations = engine.evaluate(&c).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "allowed_domains");
    }

    #[test]
    fn allowed_domains_accepts_a_domain_in_the_list() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::AllowedDomains { domains: vec!["policy".to_string()] }]);
        let c = capsule(&base_json("policy", "alice", "x"));
        assert!(engine.evaluate(&c).unwrap().is_empty());
    }

    #[test]
    fn max_confidence_ceiling_only_applies_to_the_matching_basis() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::MaxConfidenceCeiling { basis: ConfidenceBasis::PolicyMandate, max: 0.5 }]);
        let c = capsule(&base_json("policy", "alice", "x")); // basis=policy_mandate, ceiling=0.9
        let violations = engine.evaluate(&c).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "max_confidence_ceiling");
    }

    #[test]
    fn owner_pattern_enforces_a_regex() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::OwnerPattern { pattern: r"^.+@company\.example$".to_string() }]);
        let bad = capsule(&base_json("policy", "alice", "x"));
        assert_eq!(engine.evaluate(&bad).unwrap().len(), 1);

        let good = capsule(&base_json("policy", "alice@company.example", "x"));
        assert!(engine.evaluate(&good).unwrap().is_empty());
    }

    #[test]
    fn min_evidence_for_type_only_applies_to_the_matching_capsule_type() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::MinEvidenceForType { capsule_type: CapsuleType::Rule, min_evidence: 1 }]);
        let c = capsule(&base_json("policy", "alice", "x")); // capsule_type=rule, zero evidence
        let violations = engine.evaluate(&c).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "min_evidence_for_type");
    }

    #[test]
    fn forbidden_statement_phrase_is_case_insensitive() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::ForbiddenStatementPhrase { phrase: "guaranteed".to_string() }]);
        let c = capsule(&base_json("policy", "alice", "This is GUARANTEED to work."));
        let violations = engine.evaluate(&c).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "forbidden_statement_phrase");
    }

    #[test]
    fn a_clean_capsule_against_multiple_rules_has_no_violations() {
        let engine = PolicyEngine::from_rules(vec![
            PolicyRule::AllowedDomains { domains: vec!["policy".to_string()] },
            PolicyRule::OwnerPattern { pattern: r"^\w+$".to_string() },
            PolicyRule::ForbiddenStatementPhrase { phrase: "guaranteed".to_string() },
        ]);
        let c = capsule(&base_json("policy", "alice", "This is a plain policy statement."));
        assert!(engine.evaluate(&c).unwrap().is_empty());
    }

    #[test]
    fn load_reads_rules_from_a_json_file() {
        let dir = std::env::temp_dir().join(format!("capsule-policy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("policy.json");
        std::fs::write(&path, r#"{"rules": [{"type": "allowed_domains", "domains": ["policy"]}]}"#).unwrap();

        let engine = PolicyEngine::load(&path).unwrap();
        let c = capsule(&base_json("marketing", "alice", "x"));
        assert_eq!(engine.evaluate(&c).unwrap().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_invalid_regex_pattern_surfaces_as_a_policy_error_not_a_panic() {
        let engine = PolicyEngine::from_rules(vec![PolicyRule::OwnerPattern { pattern: "(unclosed".to_string() }]);
        let c = capsule(&base_json("policy", "alice", "x"));
        assert!(matches!(engine.evaluate(&c), Err(PolicyError::Regex(_, _))));
    }
}
