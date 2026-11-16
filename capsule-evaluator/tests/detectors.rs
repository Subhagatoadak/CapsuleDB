//! Each detector against a crafted answer that should trigger it, and a
//! faithful answer that should not, using the repository's example
//! capsules.

use capsule_evaluator::evaluate::{DecidedBy, Support};
use capsule_evaluator::judge::FnJudge;
use capsule_evaluator::surface::{JudgeDiscretion, ResidualKind, Rule};
use capsule_evaluator::{evaluate, CapsulePack, EvaluationReport, FindingKind, NoJudge, SurfaceManifest, Verdict};

fn repo(p: &str) -> String {
    format!("{}/../{p}", env!("CARGO_MANIFEST_DIR"))
}

fn marketing() -> CapsulePack {
    CapsulePack::load(&[
        repo("examples/capsules/marketing-incrementality-estimate.json"),
        repo("examples/capsules/marketing-incrementality-counterexample.json"),
        repo("capsule-evaluator/fixtures/email-frequency-churn.json"),
    ])
    .unwrap()
}

fn retention() -> CapsulePack {
    CapsulePack::load(&[repo("examples/capsules/data-retention-90-days.json"), repo("examples/capsules/data-retention-180-days.json")]).unwrap()
}

fn run(answer: &str, pack: &CapsulePack) -> EvaluationReport {
    evaluate(answer, pack, &SurfaceManifest::default(), &NoJudge)
}

fn kinds(r: &EvaluationReport) -> Vec<FindingKind> {
    r.findings.iter().map(|f| f.kind).collect()
}

fn has(r: &EvaluationReport, k: FindingKind) -> bool {
    kinds(r).contains(&k)
}

#[test]
fn a_faithful_answer_passes() {
    let r = run(
        "Paid search campaigns produced an estimated 6-9% incremental lift in conversions in Q1 2026, based on a single \
         geo-holdout experiment that has not yet been replicated. However, a preliminary audit found the holdout markets \
         were not fully randomized, which may bias the lift estimate upward; that dispute is unresolved.",
        &marketing(),
    );
    assert!(r.findings.is_empty(), "{:#?}", r.findings);
    assert_eq!(r.verdict, Verdict::Pass);
    assert_eq!(r.scores.epistemic_fidelity, 1.0);
    assert_eq!(r.scores.confidence_laundering_risk, 0.0);
    assert_eq!(r.scores.evaluation_surface_reduction, 1.0);
}

#[test]
fn inference_to_fact_and_its_companions() {
    let r = run("Paid search campaigns definitely produced a 6-9% incremental lift in conversions in Q1 2026.", &marketing());
    assert!(has(&r, FindingKind::InferenceToFact), "{:?}", kinds(&r));
    assert!(has(&r, FindingKind::ConfidenceCeilingViolation), "{:?}", kinds(&r));
    assert!(has(&r, FindingKind::MissingQualification), "{:?}", kinds(&r));
    assert!(has(&r, FindingKind::ContradictionSuppression), "{:?}", kinds(&r));
    assert_eq!(r.verdict, Verdict::Fail);
    assert!(r.scores.confidence_laundering_risk > 0.8);
}

#[test]
fn dropping_the_hedge_is_flagged_even_without_strong_words() {
    let r = run("Paid search campaigns produced a 6-9% incremental lift in conversions in Q1 2026.", &marketing());
    let f = r.findings.iter().find(|f| f.kind == FindingKind::InferenceToFact).expect("finding");
    assert_eq!(f.cue.as_deref(), Some("(no hedge)"));
}

#[test]
fn applicability_expansion_in_time_and_scope() {
    let later = run("Paid search will produce an estimated 6-9% incremental lift in conversions in Q3 2026.", &marketing());
    let f = later.findings.iter().find(|f| f.kind == FindingKind::ApplicabilityExpansion).expect("period finding");
    assert_eq!(f.cue.as_deref(), Some("Q3 2026"));

    let wider = run("The estimated 6-9% incremental lift in conversions holds across all channels.", &marketing());
    assert!(has(&wider, FindingKind::ApplicabilityExpansion), "{:?}", kinds(&wider));

    let excluded = run("Customer transaction data must be retained for no more than 180 days after account closure, even for records under an active legal hold.", &retention());
    assert!(has(&excluded, FindingKind::ApplicabilityExpansion), "{:?}", kinds(&excluded));

    let abroad = run("In the United States, customer transaction data must be retained for no more than 180 days after account closure, unless a legal hold applies.", &retention());
    assert!(has(&abroad, FindingKind::ApplicabilityExpansion), "{:?}", kinds(&abroad));
}

#[test]
fn association_turned_into_causation() {
    let r = run("Higher weekly email frequency causes lower subscriber churn.", &marketing());
    assert!(has(&r, FindingKind::AssociationToCausation), "{:?}", kinds(&r));
    let ok = run(
        "Higher weekly email frequency is associated with lower subscriber churn, an observational association from the 2025 cohort analysis.",
        &marketing(),
    );
    assert!(!has(&ok, FindingKind::AssociationToCausation), "{:?}", ok.findings);
    assert!(!has(&ok, FindingKind::MissingQualification), "{:?}", ok.findings);
}

#[test]
fn explicit_confidence_above_the_ceiling() {
    let r = run(
        "We are 95% confident that paid search campaigns produced an estimated 6-9% incremental lift in conversions in Q1 2026.",
        &marketing(),
    );
    let f = r.findings.iter().find(|f| f.kind == FindingKind::ConfidenceCeilingViolation).expect("ceiling finding");
    assert!(f.message.contains("0.95") && f.message.contains("0.60"), "{}", f.message);
    // A 95% confidence *interval* is not a confidence claim.
    let ci = run(
        "Paid search campaigns produced an estimated 6-9% incremental lift in conversions in Q1 2026 (95% confidence interval).",
        &marketing(),
    );
    assert!(!has(&ci, FindingKind::ConfidenceCeilingViolation), "{:?}", kinds(&ci));
}

#[test]
fn single_source_presented_as_independent_evidence() {
    let r = run(
        "Multiple independent studies show paid search campaigns produced an estimated 6-9% incremental lift in conversions.",
        &marketing(),
    );
    assert!(has(&r, FindingKind::EvidenceMultiplication), "{:?}", kinds(&r));
}

#[test]
fn superseded_policy_and_missing_exception() {
    let old = run(
        "Customer transaction data must be retained for no more than 90 days after account closure, unless a legal hold applies.",
        &retention(),
    );
    assert!(has(&old, FindingKind::SupersededKnowledge), "{:?}", kinds(&old));

    let missing = run("Customer transaction data must be retained for no more than 180 days after account closure.", &retention());
    assert!(has(&missing, FindingKind::MissingQualification), "{:?}", kinds(&missing));
    assert!(!has(&missing, FindingKind::SupersededKnowledge));

    let current = run(
        "Customer transaction data must be retained for no more than 180 days after account closure, unless a legal hold applies.",
        &retention(),
    );
    assert!(current.findings.is_empty(), "{:#?}", current.findings);
}

#[test]
fn numeric_mismatch_and_prohibited_overclaim() {
    let wrong = run("Paid search campaigns produced an estimated 15% incremental lift in conversions in Q1 2026.", &marketing());
    assert!(has(&wrong, FindingKind::NumericMismatch), "{:?}", kinds(&wrong));

    let over = run("More email reduces churn for newsletter subscribers.", &marketing());
    assert!(has(&over, FindingKind::ProhibitedOverclaim), "{:?}", kinds(&over));
    let negated = run("It is not established that more email reduces churn; the link is an observational association.", &marketing());
    assert!(!has(&negated, FindingKind::ProhibitedOverclaim), "{:?}", negated.findings);
}

#[test]
fn unsupported_claims_are_reported() {
    let r = run("Television advertising is the cheapest channel for reaching new customers.", &marketing());
    assert!(has(&r, FindingKind::UnsupportedClaim));
    assert_eq!(r.claims[0].support, Support::Unsupported);
}

#[test]
fn residual_questions_go_to_the_judge_only_when_the_manifest_allows() {
    let answer = "Paid search ads boosted conversions last spring.";
    let none = run(answer, &marketing());
    assert_eq!(none.claims[0].support, Support::Unresolved, "{:?}", none.claims[0]);
    assert_eq!(none.verdict, Verdict::NeedsReview);
    assert!(none.scores.evaluation_surface_reduction < 1.0);
    assert!(none.judge_calls.is_empty());

    let mut surface = SurfaceManifest::default();
    surface.judge.discretion = JudgeDiscretion::Classify;
    surface.judge.may_answer = vec![ResidualKind::ClaimSupport];
    let judge = FnJudge(|q| if q.claim.contains("boosted") { "unsupported".into() } else { "supported".into() });
    let judged = evaluate(answer, &marketing(), &surface, &judge);
    assert_eq!(judged.judge_calls.len(), 1);
    assert!(judged.judge_calls[0].question.prompt().contains("CAPSULE STATEMENT"));
    assert!(judged.decisions.iter().any(|d| d.decided_by == DecidedBy::Judge));
    assert!(has(&judged, FindingKind::UnsupportedClaim));
    assert_eq!(judged.judge, "scripted");

    // A judge answer outside the allowed labels is ignored.
    let rogue = FnJudge(|_| "9/10, great answer".into());
    let r = evaluate(answer, &marketing(), &surface, &rogue);
    assert_eq!(r.claims[0].support, Support::Unresolved);
}

#[test]
fn manifest_rules_and_check_selection() {
    let mut surface = SurfaceManifest::default();
    surface.rules.push(Rule {
        id: "cite-legal-hold".into(),
        description: "retention answers must name the legal-hold exception".into(),
        when_capsule: Some("kc://policy/data-retention/customer-data-90-days".into()),
        forbid: vec![],
        require_any: vec!["legal hold".into()],
    });
    let r = evaluate("Customer transaction data must be retained for no more than 180 days after account closure.", &retention(), &surface, &NoJudge);
    assert!(has(&r, FindingKind::RuleViolation), "{:?}", kinds(&r));

    let yaml = "id: retention-qa\nversion: 2.0.0\nchecks: [unsupported_claims, superseded_knowledge]\njudge:\n  discretion: none\n";
    let only = SurfaceManifest::from_yaml(yaml).unwrap();
    let r = evaluate("Customer transaction data must be retained for no more than 180 days after account closure.", &retention(), &only, &NoJudge);
    assert!(r.findings.is_empty(), "{:?}", kinds(&r));
    assert_eq!(r.surface_manifest, "retention-qa@2.0.0");
    assert!(SurfaceManifest::from_yaml("id: x\njudge:\n  discretion: none\n  may_answer: [claim_support]\n").is_err());
}
