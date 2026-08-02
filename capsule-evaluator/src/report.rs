use crate::detectors::{Finding, FindingKind};
use crate::judge::JudgeVerdict;
use crate::mapping::ClaimMapping;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// What the deterministic layer settled, and what's left as residual
/// surface for a judge — the roadmap's "evaluation-surface manifest."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationSurfaceManifest {
    pub total_claims: usize,
    pub claims_strongly_supported: usize,
    pub claims_ambiguous_referred_to_judge: usize,
    pub claims_unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scores {
    /// Share of claims the deterministic layer resolved on its own
    /// (strong support or flatly unsupported) without needing a judge —
    /// higher is better; a judge that has to adjudicate everything
    /// reduces nothing.
    pub evaluation_surface_reduction: f64,
    /// Share of claims with strong support and zero findings against
    /// them — higher is better.
    pub epistemic_fidelity: f64,
    /// Findings per claim from the confidence-laundering-shaped
    /// detectors (inference-to-fact, association-to-causation,
    /// confidence-ceiling violation) — lower is better, unbounded above
    /// (a single claim can rack up more than one).
    pub confidence_laundering_risk: f64,
}

fn compute_scores(manifest: &EvaluationSurfaceManifest, findings: &[Finding]) -> Scores {
    let total = manifest.total_claims.max(1) as f64;

    let evaluation_surface_reduction = (manifest.claims_strongly_supported + manifest.claims_unsupported) as f64 / total;

    let flagged_claims: HashSet<usize> = findings.iter().filter_map(|f| f.claim_index).collect();
    let epistemic_fidelity = (manifest.claims_strongly_supported.saturating_sub(flagged_claims.len())) as f64 / total;

    let laundering_findings = findings
        .iter()
        .filter(|f| matches!(f.kind, FindingKind::InferenceToFact | FindingKind::AssociationToCausation | FindingKind::ConfidenceCeilingViolation))
        .count();
    let confidence_laundering_risk = laundering_findings as f64 / total;

    Scores { evaluation_surface_reduction, epistemic_fidelity, confidence_laundering_risk }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub findings: Vec<Finding>,
    pub judge_verdicts: Vec<(usize, JudgeVerdict)>,
    pub surface: EvaluationSurfaceManifest,
    pub scores: Scores,
}

pub fn build_manifest(mappings: &[ClaimMapping]) -> EvaluationSurfaceManifest {
    let total_claims = mappings.len();
    let claims_strongly_supported = mappings.iter().filter(|m| m.strong_support().next().is_some()).count();
    let claims_ambiguous_referred_to_judge = mappings.iter().filter(|m| m.is_ambiguous()).count();
    let claims_unsupported = mappings.iter().filter(|m| m.is_unsupported()).count();
    EvaluationSurfaceManifest { total_claims, claims_strongly_supported, claims_ambiguous_referred_to_judge, claims_unsupported }
}

pub fn build_report(mappings: &[ClaimMapping], findings: Vec<Finding>, judge_verdicts: Vec<(usize, JudgeVerdict)>) -> EvaluationReport {
    let surface = build_manifest(mappings);
    let scores = compute_scores(&surface, &findings);
    EvaluationReport { findings, judge_verdicts, surface, scores }
}
