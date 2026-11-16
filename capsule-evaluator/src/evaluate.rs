//! The evaluation pipeline:
//!
//! ```text
//! answer → claim decomposition → capsule mapping → deterministic checks
//!        → bounded judge (residual questions only) → report + scores
//! ```

use crate::judge::{bounded, Judge, JudgeQuestion, JudgeVerdict};
use crate::pack::{CapsulePack, PackEntry};
use crate::surface::{Check, JudgeDiscretion, ResidualKind, SurfaceManifest};
use crate::text::*;
use capsule_core::EpistemicStatus;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    UnsupportedClaim,
    NumericMismatch,
    ApplicabilityExpansion,
    InferenceToFact,
    AssociationToCausation,
    ConfidenceCeilingViolation,
    MissingQualification,
    ContradictionSuppression,
    EvidenceMultiplication,
    ProhibitedOverclaim,
    SupersededKnowledge,
    RuleViolation,
}

impl FindingKind {
    /// Weight in the Confidence Laundering Risk Score (0 = not laundering).
    fn laundering_weight(self) -> f64 {
        match self {
            FindingKind::AssociationToCausation | FindingKind::ConfidenceCeilingViolation => 0.6,
            FindingKind::InferenceToFact | FindingKind::EvidenceMultiplication | FindingKind::ProhibitedOverclaim => 0.5,
            FindingKind::ApplicabilityExpansion | FindingKind::ContradictionSuppression => 0.35,
            FindingKind::MissingQualification => 0.25,
            _ => 0.0,
        }
    }

    /// Penalty in the Epistemic Fidelity Score.
    fn fidelity_penalty(self) -> f64 {
        match self {
            FindingKind::UnsupportedClaim | FindingKind::NumericMismatch | FindingKind::SupersededKnowledge => 1.0,
            FindingKind::ProhibitedOverclaim => 0.6,
            FindingKind::AssociationToCausation | FindingKind::ConfidenceCeilingViolation => 0.5,
            FindingKind::InferenceToFact | FindingKind::ApplicabilityExpansion => 0.4,
            FindingKind::ContradictionSuppression | FindingKind::EvidenceMultiplication => 0.35,
            FindingKind::MissingQualification | FindingKind::RuleViolation => 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub severity: Severity,
    /// Index of the answer claim, if the finding is about one claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capsule: Option<String>,
    pub message: String,
    /// The exact cue or text that triggered it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    Deterministic,
    Judge,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub check: Check,
    pub subject: String,
    pub outcome: String,
    pub decided_by: DecidedBy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub capsule: String,
    pub score: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub numeric_mismatch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Supported,
    Unsupported,
    /// Ambiguous mapping, and no judge was allowed or available.
    Unresolved,
    /// A caveat or hedge about the rest of the answer, not a new assertion.
    Qualifying,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimAssessment {
    pub claim: AnswerClaim,
    pub support: Support,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capsule: Option<Mapping>,
    /// Other capsules the claim is related to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Mapping>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expressed_confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeCall {
    pub question: JudgeQuestion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<JudgeVerdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scores {
    /// Share of evaluation decisions settled deterministically
    /// (1.0 = the judge decided nothing).
    pub evaluation_surface_reduction: f64,
    /// How faithfully the answer carries the capsules' epistemic state
    /// (1.0 = no fidelity findings).
    pub epistemic_fidelity: f64,
    /// Likelihood that the answer launders confidence (0.0 = none found).
    pub confidence_laundering_risk: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    /// No high-severity finding, but something needs a person.
    NeedsReview,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub surface_manifest: String,
    pub judge: String,
    pub verdict: Verdict,
    pub scores: Scores,
    pub claims: Vec<ClaimAssessment>,
    pub findings: Vec<Finding>,
    pub decisions: Vec<Decision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judge_calls: Vec<JudgeCall>,
    /// Capsules the answer relies on.
    pub capsules_used: Vec<String>,
}

const SUPPORT_LABELS: [&str; 3] = ["supported", "unsupported", "unclear"];
const QUAL_LABELS: [&str; 3] = ["conveyed", "missing", "unclear"];

struct Ctx<'a> {
    surface: &'a SurfaceManifest,
    judge: &'a dyn Judge,
    findings: Vec<Finding>,
    decisions: Vec<Decision>,
    judge_calls: Vec<JudgeCall>,
}

impl Ctx<'_> {
    fn decide(&mut self, check: Check, subject: impl Into<String>, outcome: impl Into<String>, by: DecidedBy) {
        self.decisions.push(Decision { check, subject: subject.into(), outcome: outcome.into(), decided_by: by });
    }

    fn find(&mut self, kind: FindingKind, severity: Severity, claim: Option<usize>, capsule: Option<&str>, message: String, cue: Option<&str>) {
        self.findings.push(Finding {
            kind,
            severity,
            claim,
            capsule: capsule.map(str::to_string),
            message,
            cue: cue.map(|c| c.trim().to_string()),
        });
    }

    /// Asks the judge a residual question if the manifest allows it.
    fn residual(&mut self, kind: ResidualKind, q: JudgeQuestion) -> Option<String> {
        let discretion = self.surface.judge.discretion;
        if !self.surface.judge_may(kind) || self.judge_calls.len() >= self.surface.judge.max_calls {
            return None;
        }
        let verdict = bounded(&q, self.judge.ask(&q, discretion), discretion);
        let label = verdict.as_ref().map(|v| v.label.clone()).filter(|l| l != "unclear");
        self.judge_calls.push(JudgeCall { question: q, verdict });
        label
    }
}

fn score(claim_tokens: &BTreeSet<String>, claim_numbers: &BTreeSet<String>, e: &PackEntry) -> Mapping {
    let mut s = overlap(claim_tokens, &e.tokens);
    let capsule_numbers = numbers(&e.capsule.statement);
    let mut mismatch = false;
    if !claim_numbers.is_empty() && !capsule_numbers.is_empty() {
        if claim_numbers.intersection(&capsule_numbers).next().is_some() {
            s += 0.15;
        } else {
            mismatch = true;
        }
    }
    Mapping { capsule: e.key.clone(), score: (s.min(1.0) * 1000.0).round() / 1000.0, numeric_mismatch: mismatch }
}

fn confidence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(\d{1,3}(?:\.\d+)?)\s*%\s*(confident|certain|confidence|sure|probability|chance|likely)(\s+intervals?)?").expect("valid regex")
    })
}

/// The confidence a claim expresses, if it expresses one.
pub fn expressed_confidence(text: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut bump = |v: f64| best = Some(best.map_or(v, |b: f64| b.max(v)));
    for c in confidence_re().captures_iter(text) {
        // "95% confidence interval" describes an estimate, not certainty.
        if c.get(3).is_some() {
            continue;
        }
        if let Ok(v) = c[1].parse::<f64>() {
            bump((v / 100.0).min(1.0));
        }
    }
    const LEVELS: &[(&[&str], f64)] = &[
        (
            &[
                " certain ", " certainly ", " definitely ", " undoubtedly ", " proven ", " proves ", " conclusively ", " guaranteed ",
                " without doubt", " established fact", " is a fact", " beyond doubt",
            ],
            0.97,
        ),
        (&[" highly likely", " very likely", " high confidence", " strong evidence", " strongly suggests", " almost certainly"], 0.85),
        (&[" likely ", " probably ", " confident "], 0.7),
        (&[" moderate confidence", " moderately confident"], 0.6),
        // Hedges ("may", "preliminary") only ever lower confidence, so they
        // never express a level that could breach a ceiling.
    ];
    for (cues, v) in LEVELS {
        if find_cue(text, cues).is_some() {
            bump(*v);
            break;
        }
    }
    best
}

const JURISDICTIONS: &[(&str, &str)] = &[
    (" india ", "IN"),
    (" indian ", "IN"),
    (" eu ", "EU"),
    (" europe ", "EU"),
    (" european ", "EU"),
    (" united states ", "US"),
    (" usa ", "US"),
    (" u s ", "US"),
    (" uk ", "GB"),
    (" united kingdom ", "GB"),
    (" germany ", "DE"),
    (" france ", "FR"),
    (" singapore ", "SG"),
    (" japan ", "JP"),
    (" canada ", "CA"),
    (" australia ", "AU"),
    (" brazil ", "BR"),
    (" china ", "CN"),
    (" globally ", "*"),
    (" worldwide ", "*"),
];

fn jurisdictions_in(text: &str) -> Vec<(&'static str, &'static str)> {
    let padded = format!(" {} ", text.to_lowercase().replace(['.', ',', ';', ':', '(', ')'], " "));
    JURISDICTIONS.iter().filter(|(cue, _)| padded.contains(cue)).map(|(c, j)| (c.trim(), *j)).collect()
}

fn hedged_status(s: EpistemicStatus) -> bool {
    !matches!(s, EpistemicStatus::Observed | EpistemicStatus::PolicyDefined)
}

fn negated(text: &str) -> bool {
    find_cue(text, NEGATION_CUES).is_some()
}

/// Runs the full evaluation.
pub fn evaluate(answer: &str, pack: &CapsulePack, surface: &SurfaceManifest, judge: &dyn Judge) -> EvaluationReport {
    let mut cx = Ctx { surface, judge, findings: vec![], decisions: vec![], judge_calls: vec![] };
    let t = surface.thresholds.clone();
    let claims = decompose(answer);
    let answer_tokens = content_tokens(answer);
    let mut assessments = Vec::new();
    // capsule key -> claim indexes relying on it
    let mut used: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    // --- mapping and support -------------------------------------------
    for claim in &claims {
        let ct = content_tokens(&claim.text);
        let cn = numbers(&claim.text);
        let mut maps: Vec<Mapping> = pack.entries.iter().map(|e| score(&ct, &cn, e)).collect();
        maps.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.numeric_mismatch.cmp(&b.numeric_mismatch)));
        let best = maps.first().cloned().filter(|m| m.score >= t.related);
        let related: Vec<Mapping> = maps.iter().skip(1).filter(|m| m.score >= t.related).cloned().collect();
        let subject = format!("claim {}", claim.index);
        // A short caveat clause ("that dispute is unresolved") qualifies
        // the answer rather than asserting new knowledge.
        let qualifying = best.is_none()
            && ct.len() <= 4
            && (find_cue(&claim.text, DISPUTE_CUES).is_some() || find_cue(&claim.text, HEDGE_CUES).is_some()
                || ct.iter().any(|t| t.starts_with("unresolv") || t.starts_with("disput")));
        let support = match &best {
            None if qualifying => {
                cx.decide(Check::UnsupportedClaims, &subject, "qualifying_clause", DecidedBy::Deterministic);
                Support::Qualifying
            }
            None => {
                if surface.runs(Check::UnsupportedClaims) {
                    cx.find(
                        FindingKind::UnsupportedClaim,
                        Severity::High,
                        Some(claim.index),
                        None,
                        format!("no capsule in the pack supports \"{}\"", claim.text),
                        None,
                    );
                }
                cx.decide(Check::UnsupportedClaims, &subject, "unsupported", DecidedBy::Deterministic);
                Support::Unsupported
            }
            Some(m) if m.score >= t.support && m.numeric_mismatch => {
                cx.find(
                    FindingKind::NumericMismatch,
                    Severity::High,
                    Some(claim.index),
                    Some(&m.capsule),
                    format!("figures in \"{}\" do not appear in the capsule statement", claim.text),
                    None,
                );
                cx.decide(Check::UnsupportedClaims, &subject, "numeric_mismatch", DecidedBy::Deterministic);
                Support::Unsupported
            }
            Some(m) if m.score >= t.support => {
                cx.decide(Check::UnsupportedClaims, &subject, "supported", DecidedBy::Deterministic);
                Support::Supported
            }
            Some(m) => {
                let statement = pack.get(&m.capsule).map(|e| e.capsule.statement.clone()).unwrap_or_default();
                let q = JudgeQuestion {
                    kind: ResidualKind::ClaimSupport,
                    claim: claim.text.clone(),
                    capsule_statement: statement,
                    qualification: None,
                    labels: SUPPORT_LABELS.iter().map(|s| s.to_string()).collect(),
                };
                match cx.residual(ResidualKind::ClaimSupport, q).as_deref() {
                    Some("supported") => {
                        cx.decide(Check::UnsupportedClaims, &subject, "supported", DecidedBy::Judge);
                        Support::Supported
                    }
                    Some(_) => {
                        cx.find(
                            FindingKind::UnsupportedClaim,
                            Severity::High,
                            Some(claim.index),
                            Some(&m.capsule),
                            format!("judge found \"{}\" unsupported by the capsule", claim.text),
                            None,
                        );
                        cx.decide(Check::UnsupportedClaims, &subject, "unsupported", DecidedBy::Judge);
                        Support::Unsupported
                    }
                    None => {
                        cx.decide(Check::UnsupportedClaims, &subject, "unresolved", DecidedBy::Unresolved);
                        Support::Unresolved
                    }
                }
            }
        };
        let expressed = expressed_confidence(&claim.text);
        if let (Some(m), Support::Supported) = (&best, support) {
            used.entry(m.capsule.clone()).or_default().push(claim.index);
        }
        assessments.push(ClaimAssessment { claim: claim.clone(), support, capsule: best, related, expressed_confidence: expressed });
    }

    // --- claim-level epistemic checks -----------------------------------
    for a in &assessments {
        if a.support != Support::Supported {
            continue;
        }
        let Some(m) = &a.capsule else { continue };
        let Some(e) = pack.get(&m.capsule) else { continue };
        let i = Some(a.claim.index);
        let text = &a.claim.text;
        let cap = Some(e.key.as_str());
        let subject = format!("claim {} / {}", a.claim.index, e.key);

        if surface.runs(Check::SupersededKnowledge) {
            match &e.superseded_by {
                Some(by) => {
                    cx.find(
                        FindingKind::SupersededKnowledge,
                        Severity::High,
                        i,
                        cap,
                        format!("relies on {} which is no longer in force (superseded by {by})", e.key),
                        None,
                    );
                    cx.decide(Check::SupersededKnowledge, &subject, "superseded", DecidedBy::Deterministic);
                }
                None => cx.decide(Check::SupersededKnowledge, &subject, "current", DecidedBy::Deterministic),
            }
        }

        if surface.runs(Check::ApplicabilityExpansion) {
            let mut hit: Option<(String, String)> = None;
            if let Some(app) = &e.capsule.applicability {
                for p in periods(text) {
                    let before = app.valid_from.as_deref().is_some_and(|f| p.until.as_str() < f);
                    let after = app.valid_until.as_deref().is_some_and(|u| p.from.as_str() > u);
                    if before || after {
                        hit = Some((
                            format!(
                                "applies the capsule to {} outside its validity ({} to {})",
                                p.label,
                                app.valid_from.as_deref().unwrap_or("open"),
                                app.valid_until.as_deref().unwrap_or("open")
                            ),
                            p.label.clone(),
                        ));
                        break;
                    }
                }
                if hit.is_none() && !app.jurisdictions.is_empty() {
                    if let Some((cue, j)) = jurisdictions_in(text).into_iter().find(|(_, j)| !app.jurisdictions.iter().any(|x| x == j)) {
                        hit = Some((format!("extends the capsule to '{cue}' ({j}); it applies to {}", app.jurisdictions.join(", ")), cue.into()));
                    }
                }
                let limited = find_cue(text, LIMITING_CUES).is_some();
                if hit.is_none() && !limited {
                    for ex in &app.exclusions {
                        let ex_tokens: BTreeSet<String> = content_tokens(ex)
                            .into_iter()
                            .filter(|w| !["validat", "appli", "apply", "record", "subject", "case"].contains(&w.as_str()))
                            .collect();
                        let claim_tokens = content_tokens(text);
                        let shared: Vec<&String> = ex_tokens.intersection(&claim_tokens).collect();
                        if shared.len() >= 2 && shared.len() * 2 >= ex_tokens.len() {
                            hit = Some((format!("covers an excluded case: \"{ex}\""), shared.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")));
                            break;
                        }
                    }
                }
                let scoped = app.valid_until.is_some() || !app.conditions.is_empty() || !app.exclusions.is_empty();
                if hit.is_none() && scoped && !limited {
                    if let Some(cue) = find_cue(text, GENERALIZATION_CUES) {
                        hit = Some((format!("generalizes beyond the capsule's scope with '{}'", cue.trim()), cue.into()));
                    }
                }
            }
            match hit {
                Some((msg, cue)) => {
                    cx.find(FindingKind::ApplicabilityExpansion, Severity::High, i, cap, msg, Some(&cue));
                    cx.decide(Check::ApplicabilityExpansion, &subject, "expanded", DecidedBy::Deterministic);
                }
                None => cx.decide(Check::ApplicabilityExpansion, &subject, "within_scope", DecidedBy::Deterministic),
            }
        }

        let inflation = capsule_compiler::detect_inflation(&e.capsule.statement, text);

        if surface.runs(Check::InferenceToFact) {
            let mut hit: Option<(String, String)> = None;
            if hedged_status(e.capsule.epistemic_status) {
                if let Some(f) = inflation.iter().find(|f| f.pattern != "associated with -> causes") {
                    hit = Some((format!("hardens '{}' into '{}'", f.weak_phrase, f.inflated_phrase), f.pattern.to_string()));
                } else if let Some(cue) = find_cue(text, FACT_CUES) {
                    hit = Some((
                        format!("states as established fact a claim the capsule marks as {:?}", e.capsule.epistemic_status).to_lowercase(),
                        cue.into(),
                    ));
                } else if find_cue(text, HEDGE_CUES).is_none() && find_cue(&e.capsule.statement, HEDGE_CUES).is_some() {
                    let classified = capsule_compiler::classify::classify_epistemic_status(text);
                    if matches!(classified, EpistemicStatus::Unknown | EpistemicStatus::Observed) {
                        hit = Some((
                            format!("drops the hedge on a {:?} claim and states it flatly", e.capsule.epistemic_status).to_lowercase(),
                            "(no hedge)".into(),
                        ));
                    }
                }
            }
            match hit {
                Some((msg, cue)) => {
                    let sev = if cue == "(no hedge)" { Severity::Medium } else { Severity::High };
                    cx.find(FindingKind::InferenceToFact, sev, i, cap, msg, Some(&cue));
                    cx.decide(Check::InferenceToFact, &subject, "hardened", DecidedBy::Deterministic);
                }
                None => cx.decide(Check::InferenceToFact, &subject, "status_preserved", DecidedBy::Deterministic),
            }
        }

        if surface.runs(Check::AssociationToCausation) {
            let evidence_text: String = std::iter::once(e.capsule.statement.clone())
                .chain(e.capsule.evidence.iter().filter_map(|ev| ev.excerpt.clone()))
                .collect::<Vec<_>>()
                .join(" ");
            let causal = find_cue(text, CAUSAL_CUES);
            let assoc = find_cue(&evidence_text, ASSOCIATIVE_CUES);
            let lower_evidence = evidence_text.to_lowercase();
            let design_negated = ["not randomi", "non-randomi", "not a randomi", "unrandomi", "observational"]
                .iter()
                .any(|n| lower_evidence.contains(n));
            let design = find_cue(&evidence_text, CAUSAL_DESIGN_CUES).filter(|_| !design_negated);
            let pattern = inflation.iter().any(|f| f.pattern == "associated with -> causes");
            match (causal, assoc, design) {
                (Some(c), Some(a), None) if !negated(text) || pattern => {
                    cx.find(
                        FindingKind::AssociationToCausation,
                        Severity::High,
                        i,
                        cap,
                        format!("turns an association ('{}') into causation", a.trim()),
                        Some(c),
                    );
                    cx.decide(Check::AssociationToCausation, &subject, "causal_overreach", DecidedBy::Deterministic);
                }
                _ => cx.decide(Check::AssociationToCausation, &subject, "ok", DecidedBy::Deterministic),
            }
        }

        if surface.runs(Check::ConfidenceCeiling) {
            match a.expressed_confidence {
                Some(c) if c > e.effective_ceiling + t.ceiling_tolerance => {
                    let bound = match &e.ceiling_bound_by {
                        Some(by) => format!(" (bounded by its dependency {by})"),
                        None => String::new(),
                    };
                    cx.find(
                        FindingKind::ConfidenceCeilingViolation,
                        Severity::High,
                        i,
                        cap,
                        format!("expresses confidence {c:.2} above the capsule's ceiling {:.2}{bound}", e.effective_ceiling),
                        None,
                    );
                    cx.decide(Check::ConfidenceCeiling, &subject, "exceeds_ceiling", DecidedBy::Deterministic);
                }
                _ => cx.decide(Check::ConfidenceCeiling, &subject, "within_ceiling", DecidedBy::Deterministic),
            }
        }

        if surface.runs(Check::ProhibitedOverclaims) {
            let ct = content_tokens(text);
            let mut hit = None;
            if let Some(contract) = &e.capsule.evaluation_contract {
                for o in &contract.prohibited_overclaims {
                    let cov = coverage(&content_tokens(o), &ct);
                    let parity = negated(text) == negated(o);
                    if cov >= t.overclaim && parity {
                        hit = Some(o.clone());
                        break;
                    }
                }
            }
            match hit {
                Some(o) => {
                    cx.find(FindingKind::ProhibitedOverclaim, Severity::High, i, cap, format!("restates a prohibited overclaim: \"{o}\""), None);
                    cx.decide(Check::ProhibitedOverclaims, &subject, "overclaim", DecidedBy::Deterministic);
                }
                None => cx.decide(Check::ProhibitedOverclaims, &subject, "ok", DecidedBy::Deterministic),
            }
        }
    }

    // --- capsule-level checks -------------------------------------------
    let used_keys: Vec<String> = used.keys().cloned().collect();
    for k in &used_keys {
        let Some(e) = pack.get(k) else { continue };
        if surface.runs(Check::MissingQualifications) {
            let quals = e.capsule.evaluation_contract.as_ref().map(|c| c.mandatory_qualifications.clone()).unwrap_or_default();
            for q in quals {
                let subject = format!("{k} / \"{q}\"");
                let literal = answer.to_lowercase().contains(&q.to_lowercase());
                let cov = coverage(&content_tokens(&q), &answer_tokens);
                if literal || cov >= t.qualification {
                    cx.decide(Check::MissingQualifications, subject, "conveyed", DecidedBy::Deterministic);
                    continue;
                }
                let verdict = if cov >= t.qualification_floor {
                    let question = JudgeQuestion {
                        kind: ResidualKind::QualificationConveyed,
                        claim: answer.to_string(),
                        capsule_statement: e.capsule.statement.clone(),
                        qualification: Some(q.clone()),
                        labels: QUAL_LABELS.iter().map(|s| s.to_string()).collect(),
                    };
                    cx.residual(ResidualKind::QualificationConveyed, question).map(|l| (l == "conveyed", DecidedBy::Judge))
                } else {
                    Some((false, DecidedBy::Deterministic))
                };
                match verdict {
                    Some((true, by)) => cx.decide(Check::MissingQualifications, subject, "conveyed", by),
                    Some((false, by)) => {
                        cx.find(
                            FindingKind::MissingQualification,
                            Severity::Medium,
                            used[k].first().copied(),
                            Some(k),
                            format!("omits the mandatory qualification \"{q}\""),
                            None,
                        );
                        cx.decide(Check::MissingQualifications, subject, "missing", by);
                    }
                    None => cx.decide(Check::MissingQualifications, subject, "unresolved", DecidedBy::Unresolved),
                }
            }
        }

        if surface.runs(Check::ContradictionSuppression) {
            for (other, description) in pack.unresolved_contradictions(e) {
                let subject = format!("{k} vs {}", other.key);
                let mentioned = claims.iter().any(|c| overlap(&content_tokens(&c.text), &other.tokens) >= t.related);
                let acknowledged = find_cue(answer, DISPUTE_CUES).is_some() && coverage(&other.tokens, &answer_tokens) >= 0.2;
                if mentioned || acknowledged {
                    cx.decide(Check::ContradictionSuppression, subject, "surfaced", DecidedBy::Deterministic);
                } else {
                    cx.find(
                        FindingKind::ContradictionSuppression,
                        Severity::High,
                        used[k].first().copied(),
                        Some(k),
                        format!("does not surface the unresolved contradiction from {}: {description}", other.key),
                        None,
                    );
                    cx.decide(Check::ContradictionSuppression, subject, "suppressed", DecidedBy::Deterministic);
                }
            }
        }
    }

    if surface.runs(Check::EvidenceMultiplication) && !used_keys.is_empty() {
        let sources: BTreeSet<String> = used_keys.iter().filter_map(|k| pack.get(k)).flat_map(|e| e.evidence_sources.clone()).collect();
        let total: usize = used_keys.iter().filter_map(|k| pack.get(k)).map(|e| e.evidence_sources.len()).sum();
        match find_cue(answer, INDEPENDENCE_CUES) {
            Some(cue) if sources.len() <= 1 || total > sources.len() => {
                cx.find(
                    FindingKind::EvidenceMultiplication,
                    Severity::High,
                    None,
                    None,
                    format!(
                        "presents the evidence as independent or multiple ('{}'), but it rests on {} distinct source(s)",
                        cue.trim(),
                        sources.len()
                    ),
                    Some(cue),
                );
                cx.decide(Check::EvidenceMultiplication, "answer", "multiplied", DecidedBy::Deterministic);
            }
            _ => cx.decide(Check::EvidenceMultiplication, "answer", "ok", DecidedBy::Deterministic),
        }
    }

    if surface.runs(Check::Rules) {
        let lower = answer.to_lowercase();
        for r in &surface.rules {
            let applies = match &r.when_capsule {
                Some(c) => used_keys.iter().any(|k| k == c || k.split('@').next() == Some(c.as_str())),
                None => true,
            };
            if !applies {
                continue;
            }
            let forbidden = r.forbid.iter().find(|f| lower.contains(&f.to_lowercase()));
            let missing = !r.require_any.is_empty() && !r.require_any.iter().any(|p| lower.contains(&p.to_lowercase()));
            if let Some(f) = forbidden {
                cx.find(FindingKind::RuleViolation, Severity::High, None, r.when_capsule.as_deref(), format!("{}: {}", r.id, r.description), Some(f));
                cx.decide(Check::Rules, &r.id, "violated", DecidedBy::Deterministic);
            } else if missing {
                cx.find(FindingKind::RuleViolation, Severity::Medium, None, r.when_capsule.as_deref(), format!("{}: {}", r.id, r.description), None);
                cx.decide(Check::Rules, &r.id, "violated", DecidedBy::Deterministic);
            } else {
                cx.decide(Check::Rules, &r.id, "satisfied", DecidedBy::Deterministic);
            }
        }
    }

    // --- scores ----------------------------------------------------------
    let det = cx.decisions.iter().filter(|d| d.decided_by == DecidedBy::Deterministic).count();
    let esr = if cx.decisions.is_empty() { 1.0 } else { det as f64 / cx.decisions.len() as f64 };
    let laundering = 1.0 - cx.findings.iter().map(|f| 1.0 - f.kind.laundering_weight()).product::<f64>();
    let fidelity = {
        let mut per_claim: Vec<f64> = Vec::new();
        for a in &assessments {
            let penalty: f64 =
                cx.findings.iter().filter(|f| f.claim == Some(a.claim.index)).map(|f| f.kind.fidelity_penalty()).sum::<f64>();
            let base = match a.support {
                Support::Supported | Support::Qualifying => 1.0,
                Support::Unresolved => 0.5,
                Support::Unsupported => 0.0,
            };
            per_claim.push((base - penalty).max(0.0));
        }
        let mean = if per_claim.is_empty() { 1.0 } else { per_claim.iter().sum::<f64>() / per_claim.len() as f64 };
        let global: f64 = cx.findings.iter().filter(|f| f.claim.is_none()).map(|f| 1.0 - f.kind.fidelity_penalty()).product();
        mean * global
    };
    let round = |x: f64| (x.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;

    let high = cx.findings.iter().any(|f| f.severity == Severity::High);
    let review = !cx.findings.is_empty() || cx.decisions.iter().any(|d| d.decided_by == DecidedBy::Unresolved);
    let verdict = if high {
        Verdict::Fail
    } else if review {
        Verdict::NeedsReview
    } else {
        Verdict::Pass
    };

    EvaluationReport {
        surface_manifest: format!("{}@{}", surface.id, surface.version),
        judge: if surface.judge.discretion == JudgeDiscretion::None { "none".into() } else { judge.id() },
        verdict,
        scores: Scores {
            evaluation_surface_reduction: round(esr),
            epistemic_fidelity: round(fidelity),
            confidence_laundering_risk: round(laundering),
        },
        claims: assessments,
        findings: cx.findings,
        decisions: cx.decisions,
        judge_calls: cx.judge_calls,
        capsules_used: used_keys,
    }
}
