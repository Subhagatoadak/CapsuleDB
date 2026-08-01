//! Structural and epistemic validation (spec §5, §6, §7, §10).
//!
//! Validation never panics and never mutates the capsule; it returns a list
//! of [`ValidationIssue`]s. Callers decide what to do with `Warning`s;
//! any `Error`-severity issue means the capsule does not conform to KCP.

use crate::hashing::content_hash;
use crate::types::*;
use chrono::NaiveDate;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl ValidationIssue {
    fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, severity: Severity::Error, message: message.into() }
    }

    fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, severity: Severity::Warning, message: message.into() }
    }
}

fn logical_id_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^kc://[a-z0-9]+(-[a-z0-9]+)*/[a-z0-9]+(-[a-z0-9]+)*/[a-z0-9]+(-[a-z0-9]+)*$")
            .expect("logical id pattern is a valid regex")
    })
}

/// Runs every KCP 0.1 structural and epistemic check against `capsule` and
/// returns all issues found (empty means fully conformant).
pub fn validate(capsule: &Capsule) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    check_schema_version(capsule, &mut issues);
    check_logical_id(capsule, &mut issues);
    check_version(capsule, &mut issues);
    check_content_hash(capsule, &mut issues);
    check_epistemic_envelope(capsule, &mut issues);
    check_mandatory_evidence(capsule, &mut issues);
    check_confidence_ceiling(capsule, &mut issues);
    check_applicability(capsule, &mut issues);
    check_contradiction_markers(capsule, &mut issues);

    issues
}

/// True if `validate` produced no `Error`-severity issues.
pub fn is_valid(capsule: &Capsule) -> bool {
    !validate(capsule).iter().any(|i| i.severity == Severity::Error)
}

fn check_schema_version(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    if capsule.schema_version != SCHEMA_VERSION {
        issues.push(ValidationIssue::error(
            "schema_version",
            format!(
                "unsupported schema_version '{}': capsule-core only accepts '{}'",
                capsule.schema_version, SCHEMA_VERSION
            ),
        ));
    }
}

fn check_logical_id(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    if !logical_id_pattern().is_match(&capsule.logical_id) {
        issues.push(ValidationIssue::error(
            "logical_id",
            format!(
                "logical_id '{}' does not match kc://<domain>/<topic>/<name>",
                capsule.logical_id
            ),
        ));
    }
}

fn check_version(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    if let Err(err) = semver::Version::parse(&capsule.version) {
        issues.push(ValidationIssue::error(
            "version",
            format!("version '{}' is not valid SemVer: {err}", capsule.version),
        ));
    }
}

fn check_content_hash(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    let Some(declared) = &capsule.content_hash else {
        return;
    };
    match content_hash(capsule) {
        Ok(computed) if &computed == declared => {}
        Ok(computed) => issues.push(ValidationIssue::error(
            "content_hash",
            format!("declared content_hash '{declared}' does not match computed '{computed}'"),
        )),
        Err(err) => issues.push(ValidationIssue::error(
            "content_hash",
            format!("failed to compute content hash: {err}"),
        )),
    }
}

fn check_epistemic_envelope(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    if matches!(capsule.epistemic_status, EpistemicStatus::Disputed) && capsule.contradictions.is_empty() {
        issues.push(ValidationIssue::error(
            "epistemic_envelope",
            "epistemic_status is 'disputed' but contradictions[] is empty",
        ));
    }
}

fn check_mandatory_evidence(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    let type_requires_evidence = matches!(
        capsule.capsule_type,
        CapsuleType::Fact | CapsuleType::Rule | CapsuleType::Metric
    );
    let status_requires_evidence = matches!(
        capsule.epistemic_status,
        EpistemicStatus::Observed | EpistemicStatus::ExpertJudgement
    );

    if type_requires_evidence && status_requires_evidence {
        let has_authoritative_evidence = capsule
            .evidence
            .iter()
            .any(|e| !matches!(e.authority, EvidenceAuthority::Unverified));

        if !has_authoritative_evidence {
            issues.push(ValidationIssue::error(
                "mandatory_evidence",
                format!(
                    "capsule_type '{:?}' with epistemic_status '{:?}' requires at least one evidence entry with authority != unverified",
                    capsule.capsule_type, capsule.epistemic_status
                ),
            ));
        }
    }
}

fn check_confidence_ceiling(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    let ceiling = capsule.confidence.ceiling;

    if !(0.0..=1.0).contains(&ceiling) {
        issues.push(ValidationIssue::error(
            "confidence_ceiling_range",
            format!("confidence.ceiling {ceiling} is outside [0.0, 1.0]"),
        ));
    }

    if matches!(capsule.confidence.basis, ConfidenceBasis::Unsupported) && ceiling > 0.3 {
        issues.push(ValidationIssue::error(
            "confidence_ceiling_unsupported",
            format!(
                "confidence.basis is 'unsupported' but ceiling is {ceiling}; unsupported claims are capped at 0.3"
            ),
        ));
    }
}

fn check_applicability(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    let Some(applicability) = &capsule.applicability else {
        return;
    };

    let from = applicability
        .valid_from
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let until = applicability
        .valid_until
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    if let (Some(from), Some(until)) = (from, until) {
        if from > until {
            issues.push(ValidationIssue::error(
                "applicability_window",
                format!("valid_from ({from}) is after valid_until ({until})"),
            ));
        }
    }
}

/// Deterministic contradiction markers: capsules never silently carry
/// unresolved contradictions. This does not fail validation (an unresolved
/// contradiction can be a legitimate, disclosed state) but always surfaces
/// as a warning so callers (CLI, registry, evaluator) can flag it.
fn check_contradiction_markers(capsule: &Capsule, issues: &mut Vec<ValidationIssue>) {
    for contradiction in &capsule.contradictions {
        if matches!(contradiction.resolution_status, ResolutionStatus::Unresolved) {
            issues.push(ValidationIssue::warning(
                "unresolved_contradiction",
                format!(
                    "unresolved contradiction with '{}': {}",
                    contradiction.capsule_id, contradiction.description
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_capsule() -> Capsule {
        Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            logical_id: "kc://policy/data-retention/customer-data-90-days".to_string(),
            version: "1.0.0".to_string(),
            content_hash: None,
            capsule_type: CapsuleType::Rule,
            statement: "Retain data for 90 days.".to_string(),
            epistemic_status: EpistemicStatus::PolicyDefined,
            confidence: Confidence {
                basis: ConfidenceBasis::PolicyMandate,
                ceiling: 0.95,
                rationale: None,
            },
            lifecycle_state: LifecycleState::Published,
            evidence: vec![],
            applicability: None,
            contradictions: vec![],
            dependencies: vec![],
            governance: Governance {
                owner: "privacy-office".to_string(),
                domain: "policy".to_string(),
                permissions: vec![],
            },
            evaluation_contract: None,
        }
    }

    #[test]
    fn minimal_valid_capsule_passes() {
        let capsule = base_capsule();
        assert!(is_valid(&capsule), "{:?}", validate(&capsule));
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let mut capsule = base_capsule();
        capsule.schema_version = "kcp/9.9".to_string();
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "schema_version"));
    }

    #[test]
    fn rejects_malformed_logical_id() {
        let mut capsule = base_capsule();
        capsule.logical_id = "not-a-kc-uri".to_string();
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "logical_id"));
    }

    #[test]
    fn rejects_invalid_semver() {
        let mut capsule = base_capsule();
        capsule.version = "v1".to_string();
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "version"));
    }

    #[test]
    fn disputed_requires_contradictions() {
        let mut capsule = base_capsule();
        capsule.epistemic_status = EpistemicStatus::Disputed;
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "epistemic_envelope"));
    }

    #[test]
    fn observed_fact_requires_authoritative_evidence() {
        let mut capsule = base_capsule();
        capsule.capsule_type = CapsuleType::Fact;
        capsule.epistemic_status = EpistemicStatus::Observed;
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "mandatory_evidence"));

        capsule.evidence.push(Evidence {
            source_id: "src".to_string(),
            span: None,
            excerpt: None,
            authority: EvidenceAuthority::Unverified,
            retrieved_at: None,
        });
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "mandatory_evidence"));

        capsule.evidence.push(Evidence {
            source_id: "src2".to_string(),
            span: None,
            excerpt: None,
            authority: EvidenceAuthority::Primary,
            retrieved_at: None,
        });
        assert!(is_valid(&capsule));
    }

    #[test]
    fn unsupported_basis_caps_ceiling() {
        let mut capsule = base_capsule();
        capsule.confidence.basis = ConfidenceBasis::Unsupported;
        capsule.confidence.ceiling = 0.5;
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "confidence_ceiling_unsupported"));

        capsule.confidence.ceiling = 0.2;
        assert!(is_valid(&capsule));
    }

    #[test]
    fn ceiling_out_of_range_is_rejected() {
        let mut capsule = base_capsule();
        capsule.confidence.ceiling = 1.5;
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "confidence_ceiling_range"));
    }

    #[test]
    fn applicability_window_must_be_ordered() {
        let mut capsule = base_capsule();
        capsule.applicability = Some(Applicability {
            valid_from: Some("2026-06-01".to_string()),
            valid_until: Some("2026-01-01".to_string()),
            ..Default::default()
        });
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "applicability_window"));
    }

    #[test]
    fn unresolved_contradiction_is_a_warning_not_an_error() {
        let mut capsule = base_capsule();
        capsule.epistemic_status = EpistemicStatus::Disputed;
        capsule.contradictions.push(Contradiction {
            capsule_id: "kc://a/b/c".to_string(),
            description: "conflicting claim".to_string(),
            resolution_status: ResolutionStatus::Unresolved,
        });
        let issues = validate(&capsule);
        let marker = issues.iter().find(|i| i.code == "unresolved_contradiction").unwrap();
        assert_eq!(marker.severity, Severity::Warning);
        assert!(is_valid(&capsule));
    }

    #[test]
    fn tampered_content_hash_is_detected() {
        let mut capsule = base_capsule();
        capsule.content_hash = Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string());
        let issues = validate(&capsule);
        assert!(issues.iter().any(|i| i.code == "content_hash"));
    }
}
