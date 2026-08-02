//! Lexical-cue classification: epistemic status, qualifications,
//! exclusions, applicability conditions, and bare `kc://` dependency
//! references. All heuristic and keyword-driven — deliberately so: this
//! is the deterministic pass that runs before any (future) LLM step, per
//! the roadmap's "deterministic validation before LLM validation."

use capsule_core::EpistemicStatus;
use regex::Regex;
use std::sync::OnceLock;

/// Classifies epistemic status from lexical cues in a single claim
/// sentence. Order matters: policy language ("must"/"shall") is checked
/// before hedging language, since a sentence can contain both ("must,
/// unless...") and the binding obligation should win.
pub fn classify_epistemic_status(text: &str) -> EpistemicStatus {
    let lower = text.to_lowercase();

    if contains_any(&lower, &["must", "shall", "is required to", "is prohibited from", "is not permitted to"]) {
        EpistemicStatus::PolicyDefined
    } else if contains_any(&lower, &["we observed", "data shows", "measurements show", "recorded that", "was measured"]) {
        EpistemicStatus::Observed
    } else if contains_any(&lower, &["we believe", "in our judgement", "in our judgment", "our assessment is", "in our view"]) {
        EpistemicStatus::ExpertJudgement
    } else if contains_any(&lower, &["is modelled", "is modeled", "simulation shows", "projected to", "forecast"]) {
        EpistemicStatus::Modelled
    } else if contains_any(&lower, &["disputed", "contested", "however, other", "in contrast to"]) {
        EpistemicStatus::Disputed
    } else if contains_any(&lower, &["may ", "might ", "suggests", "appears to", "estimated", "approximately", "likely"]) {
        EpistemicStatus::Estimated
    } else {
        EpistemicStatus::Unknown
    }
}

const QUALIFIER_KEYWORDS: &[&str] = &["unless", "provided that", "only if", "subject to", "in most cases", "typically", "generally"];
const EXCLUSION_KEYWORDS: &[&str] = &["except", "excluding", "does not apply to", "other than", "with the exception of"];
const CONDITION_KEYWORDS: &[&str] = &["if ", "when ", "where "];

/// Hedging/limiting language that softens a claim without excluding a
/// specific case (spec: "pair claims with qualifications and limitations").
pub fn extract_qualifications(text: &str) -> Vec<String> {
    find_after_keywords(text, QUALIFIER_KEYWORDS)
}

/// Explicitly named exceptions (spec: "detect exceptions and exclusions").
pub fn extract_exclusions(text: &str) -> Vec<String> {
    find_after_keywords(text, EXCLUSION_KEYWORDS)
}

/// Conditional clauses that scope when a claim applies.
pub fn extract_applicability_conditions(text: &str) -> Vec<String> {
    find_after_keywords(text, CONDITION_KEYWORDS)
}

/// Bare `kc://domain/topic/name[@version]` mentions in the claim text.
/// Deliberately *not* used to populate a capsule's `dependencies[]`
/// automatically — a text mention doesn't tell us the relation
/// (`requires`/`refines`/`supersedes`), so these are surfaced for a human
/// to classify during review, not guessed.
pub fn detect_dependency_refs(text: &str) -> Vec<String> {
    kc_uri_re().find_iter(text).map(|m| m.as_str().to_string()).collect()
}

fn kc_uri_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"kc://[a-z0-9]+(?:-[a-z0-9]+)*/[a-z0-9]+(?:-[a-z0-9]+)*/[a-z0-9]+(?:-[a-z0-9]+)*(?:@\d+\.\d+\.\d+)?")
            .expect("valid regex")
    })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn find_after_keywords(text: &str, keywords: &[&str]) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut found = Vec::new();
    for keyword in keywords {
        if let Some(pos) = lower.find(keyword) {
            let snippet = text[pos..].trim().trim_end_matches('.').to_string();
            if !found.contains(&snippet) {
                found.push(snippet);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_policy_language() {
        assert_eq!(classify_epistemic_status("Data must be retained for 90 days."), EpistemicStatus::PolicyDefined);
    }

    #[test]
    fn classifies_observed_language() {
        assert_eq!(classify_epistemic_status("We observed a 6% increase in conversions."), EpistemicStatus::Observed);
    }

    #[test]
    fn classifies_estimated_language() {
        assert_eq!(classify_epistemic_status("The lift may be approximately 6-9%."), EpistemicStatus::Estimated);
    }

    #[test]
    fn defaults_to_unknown() {
        assert_eq!(classify_epistemic_status("The report was published on Tuesday."), EpistemicStatus::Unknown);
    }

    #[test]
    fn extracts_qualifications() {
        let quals = extract_qualifications("Retention is 90 days unless a legal hold applies.");
        assert_eq!(quals, vec!["unless a legal hold applies"]);
    }

    #[test]
    fn extracts_exclusions() {
        let exclusions = extract_exclusions("Applies to all staff except contractors.");
        assert_eq!(exclusions, vec!["except contractors"]);
    }

    #[test]
    fn extracts_conditions() {
        let conditions = extract_applicability_conditions("If the account is closed, data must be deleted.");
        assert_eq!(conditions, vec!["If the account is closed, data must be deleted"]);
    }

    #[test]
    fn detects_kc_uri_references() {
        let refs = detect_dependency_refs("This rule refines kc://policy/retention/base-rule@1.0.0 for EU customers.");
        assert_eq!(refs, vec!["kc://policy/retention/base-rule@1.0.0"]);
    }

    #[test]
    fn no_false_positive_dependency_refs() {
        assert!(detect_dependency_refs("No references here.").is_empty());
    }
}
