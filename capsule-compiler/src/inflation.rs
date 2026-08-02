//! Deterministic confidence-laundering ("inflation") detection — the
//! transformations the top-level README calls out under "Confidence
//! laundering controls": tentative language hardening into certain
//! language as it moves through systems.
//!
//! This compares two phrasings of the *same* claim (e.g. a source excerpt
//! against an edited/summarized capsule statement) and flags known
//! weak-to-strong substitutions. The Phase 3 MVP pipeline extracts claims
//! verbatim (no paraphrasing), so it never triggers on its own output;
//! it's exposed for human review workflows and for `capsule-evaluator`
//! (Phase 6) to run whenever a capsule's statement is edited relative to
//! its evidence.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InflationFinding {
    pub weak_phrase: String,
    pub inflated_phrase: String,
    pub pattern: &'static str,
}

const LAUNDERING_PATTERNS: &[(&str, &str, &str)] = &[
    ("may", "does", "may -> does"),
    ("suggests", "proves", "suggests -> proves"),
    ("associated with", "causes", "associated with -> causes"),
    ("estimated", "observed", "estimated -> observed"),
    ("moderate confidence", "certain", "moderate confidence -> certain"),
    ("limited evidence", "established evidence", "limited evidence -> established evidence"),
];

/// Flags weak-to-strong phrase substitutions between an `original` source
/// phrasing and a `revised` (e.g. compiled or edited) phrasing. A finding
/// requires the weak phrase to appear in `original`, the strong phrase to
/// appear in `revised`, and the weak phrase to be *absent* from `revised`
/// — i.e. the qualifier was dropped, not merely supplemented.
pub fn detect_inflation(original: &str, revised: &str) -> Vec<InflationFinding> {
    let original_lower = original.to_lowercase();
    let revised_lower = revised.to_lowercase();

    LAUNDERING_PATTERNS
        .iter()
        .filter(|(weak, strong, _)| original_lower.contains(weak) && revised_lower.contains(strong) && !revised_lower.contains(weak))
        .map(|(weak, strong, pattern)| InflationFinding {
            weak_phrase: weak.to_string(),
            inflated_phrase: strong.to_string(),
            pattern,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_may_to_does() {
        let findings = detect_inflation("The drug may reduce symptoms.", "The drug does reduce symptoms.");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, "may -> does");
    }

    #[test]
    fn detects_association_to_causation() {
        let findings = detect_inflation("Smoking is associated with the disease.", "Smoking causes the disease.");
        assert!(findings.iter().any(|f| f.pattern == "associated with -> causes"));
    }

    #[test]
    fn no_finding_when_qualifier_is_retained() {
        let findings = detect_inflation("This may cause harm.", "This may cause harm and does so reliably in trials.");
        assert!(findings.is_empty());
    }

    #[test]
    fn no_finding_when_phrasing_is_unchanged() {
        let findings = detect_inflation("Limited evidence suggests a link.", "Limited evidence suggests a link.");
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_estimated_to_observed() {
        let findings = detect_inflation("The lift was estimated at 6%.", "The lift was observed at 6%.");
        assert!(findings.iter().any(|f| f.pattern == "estimated -> observed"));
    }
}
