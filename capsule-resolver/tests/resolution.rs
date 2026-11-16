//! Resolution scenarios over the repository's example capsules, from
//! files and from a real `capsule-registry`.

use capsule_core::{Capsule, CapsuleType, Dependency, DependencyRelation, LifecycleState};
use capsule_registry::Registry;
use capsule_resolver::request::{EvidenceGate, Requester};
use capsule_resolver::{parse_cql, Include, PackSource, RegistrySource, ResolutionRequest, Resolver, Role};

fn example(name: &str) -> Capsule {
    let p = format!("{}/../examples/capsules/{name}.json", env!("CARGO_MANIFEST_DIR"));
    capsule_core::parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn all() -> PackSource {
    PackSource(vec![
        example("data-retention-90-days"),
        example("data-retention-180-days"),
        example("marketing-incrementality-estimate"),
        example("marketing-incrementality-counterexample"),
    ])
}

const README_QUERY: &str = r#"RESOLVE policy_rule
ABOUT "customer data retention"
FOR purpose "privacy review"
IN jurisdiction "India"
AS OF "2026-08-01"
REQUIRE status PUBLISHED
INCLUDE exceptions
INCLUDE contradictions
WITH dependencies"#;

fn keys(b: &capsule_resolver::ResolutionBundle) -> Vec<(String, Role)> {
    b.resolved.iter().map(|r| (r.key.clone(), r.role)).collect()
}

#[test]
fn the_readme_query_resolves_the_policy_in_force() {
    let b = Resolver::new("2026-11-16").resolve(&parse_cql(README_QUERY).unwrap(), &all()).unwrap();
    assert_eq!(keys(&b), vec![("kc://policy/data-retention/customer-data-90-days@2.0.0".to_string(), Role::Primary)]);
    // The plan explains why the 1.0.0 policy was not used.
    assert!(b.plan.iter().any(|p| p.capsule.ends_with("@1.0.0") && p.step == "temporal" && p.reason.contains("newer")), "{:#?}", b.plan);
    // The marketing capsules were considered and rejected as irrelevant.
    assert!(b.plan.iter().any(|p| p.capsule.contains("marketing") && p.step != "ordering"));
    // The condition is reported, not silently assumed.
    assert_eq!(b.resolved[0].conditions_unverified, vec!["account has been formally closed"]);
    assert!(b.known_gaps.is_empty(), "{:?}", b.known_gaps);
}

#[test]
fn as_of_selects_the_version_valid_then() {
    let mut q = parse_cql(README_QUERY).unwrap();
    q.as_of = Some("2026-03-01".into());
    let b = Resolver::new("2026-11-16").resolve(&q, &all()).unwrap();
    assert_eq!(keys(&b)[0].0, "kc://policy/data-retention/customer-data-90-days@1.0.0");
    assert!(b.plan.iter().any(|p| p.capsule.ends_with("@2.0.0") && p.reason.contains("not valid on 2026-03-01")));
}

#[test]
fn applicability_and_context() {
    let mut q = parse_cql(README_QUERY).unwrap();
    q.jurisdiction = Some("Germany".into());
    let b = Resolver::new("2026-11-16").resolve(&q, &all()).unwrap();
    assert!(b.resolved.is_empty());
    assert_eq!(b.known_gaps[0].kind, "no_match");
    assert!(b.plan.iter().any(|p| p.step == "applicability" && p.reason.contains("not DE")));

    // A context that establishes the condition clears it; one that falls
    // under the exclusion removes the capsule.
    let mut q = parse_cql(README_QUERY).unwrap();
    q.context = vec!["the account has been formally closed".into()];
    let b = Resolver::new("2026-11-16").resolve(&q, &all()).unwrap();
    assert!(b.resolved[0].conditions_unverified.is_empty());
    q.context.push("records are subject to an active legal hold".into());
    let b = Resolver::new("2026-11-16").resolve(&q, &all()).unwrap();
    assert!(b.resolved.is_empty(), "{:?}", keys(&b));
}

#[test]
fn contradictions_are_bundled_with_what_they_contradict() {
    let mut counter = example("marketing-incrementality-counterexample");
    counter.applicability.as_mut().unwrap().valid_from = Some("2026-03-01".into());
    let src = PackSource(vec![example("marketing-incrementality-estimate"), counter]);
    let q = parse_cql(
        "RESOLVE metric\nABOUT \"paid search incremental lift\"\nFOR purpose \"budget planning\"\nAS OF \"2026-03-15\"\nINCLUDE contradictions\nINCLUDE exceptions",
    )
    .unwrap();
    let b = Resolver::new("2026-11-16").resolve(&q, &src).unwrap();
    assert_eq!(b.resolved[0].role, Role::Primary);
    assert_eq!(b.resolved[1].role, Role::Exception, "{:?}", keys(&b));
    assert_eq!(b.contradictions.len(), 1);
    assert!(b.contradictions[0].description.contains("randomization"));

    // Before the audit existed, there is nothing to contradict.
    let mut early = q.clone();
    early.as_of = Some("2026-02-01".into());
    let b = Resolver::new("2026-11-16").resolve(&early, &src).unwrap();
    assert_eq!(b.resolved.len(), 1);
    assert!(b.contradictions.is_empty());
}

#[test]
fn evidence_gate_and_permissions() {
    let mut counter = example("marketing-incrementality-counterexample");
    counter.capsule_type = CapsuleType::Metric; // make it a direct candidate
    let src = PackSource(vec![counter]);
    let mut q = ResolutionRequest::new("budget planning", "paid search lift randomized audit");
    q.as_of = Some("2026-06-01".into());
    q.evidence_gate = EvidenceGate::Primary;
    let b = Resolver::new("2026-11-16").resolve(&q, &src).unwrap();
    assert!(b.resolved.is_empty());
    assert!(b.known_gaps.iter().any(|g| g.kind == "evidence_insufficient"), "{:?}", b.known_gaps);

    let mut q = parse_cql(README_QUERY).unwrap();
    q.requester = Some(Requester { id: "svc:indexer".into(), permissions: vec!["write".into()], domains: vec![] });
    let b = Resolver::new("2026-11-16").resolve(&q, &all()).unwrap();
    assert!(b.resolved.is_empty());
    assert_eq!(b.plan.iter().filter(|p| p.step == "permission").count(), 4);
}

#[test]
fn dependencies_are_completed_and_bound_the_ceiling() {
    let mut rule = example("data-retention-180-days");
    rule.dependencies = vec![Dependency { capsule_id: "kc://marketing/incrementality/paid-search-lift-q1-2026".into(), relation: DependencyRelation::Requires }];
    let mut dep = example("marketing-incrementality-estimate");
    dep.applicability.as_mut().unwrap().valid_until = None;
    let src = PackSource(vec![rule, dep]);
    let b = Resolver::new("2026-11-16").resolve(&parse_cql(README_QUERY).unwrap(), &src).unwrap();
    assert_eq!(keys(&b)[1].1, Role::Dependency, "{:?}", keys(&b));
    assert_eq!(b.resolved[0].effective_ceiling, 0.6);

    // A dependency that is not in force is a known gap.
    let mut rule = example("data-retention-180-days");
    rule.dependencies = vec![Dependency { capsule_id: "kc://policy/missing/rule".into(), relation: DependencyRelation::Requires }];
    let b = Resolver::new("2026-11-16").resolve(&parse_cql(README_QUERY).unwrap(), &PackSource(vec![rule])).unwrap();
    assert!(b.known_gaps.iter().any(|g| g.kind == "unmet_dependency"));
}

#[test]
fn replay_detects_drift() {
    let r = Resolver::new("2026-11-16");
    let q = parse_cql(README_QUERY).unwrap();
    let b = r.resolve(&q, &all()).unwrap();
    let same = r.replay(&b, &all()).unwrap();
    assert!(same.identical && !same.source_changed);

    // The 2.0.0 policy is revoked: the same request now resolves differently.
    let mut revoked = example("data-retention-180-days");
    revoked.lifecycle_state = LifecycleState::Revoked;
    let changed = PackSource(vec![example("data-retention-90-days"), revoked]);
    let drift = r.replay(&b, &changed).unwrap();
    assert!(!drift.identical && drift.source_changed);
    assert!(drift.differences.iter().any(|d| d.contains("no longer resolved") && d.contains("2.0.0")), "{:?}", drift.differences);
    assert!(drift.differences.iter().any(|d| d.contains("newly resolved") && d.contains("1.0.0")), "{:?}", drift.differences);
}

#[test]
fn resolves_from_a_registry_with_its_lifecycle_state() {
    let mut reg = Registry::open_in_memory().unwrap();
    for name in ["data-retention-90-days", "data-retention-180-days"] {
        let mut c = example(name);
        c.lifecycle_state = LifecycleState::Draft;
        reg.register(&c, "owner:legal").unwrap();
        for s in [LifecycleState::InReview, LifecycleState::Approved] {
            reg.transition(&c.logical_id, &c.version, s, "owner:legal", None).unwrap();
        }
        reg.publish(&c.logical_id, &c.version, "owner:legal").unwrap();
    }
    let mut q = parse_cql(README_QUERY).unwrap();
    q.as_of = Some("2026-03-01".into());
    // 1.0.0 was superseded when 2.0.0 was published, so in March nothing
    // published is valid; the registry's state wins over the document's.
    let b = Resolver::new("2026-11-16").resolve(&q, &RegistrySource(&reg)).unwrap();
    assert!(b.resolved.is_empty(), "{:?}", keys(&b));
    assert!(b.plan.iter().any(|p| p.step == "status" && p.reason.contains("superseded")), "{:#?}", b.plan);
    q.include = vec![Include::Dependencies];
    q.as_of = Some("2026-08-01".into());
    let b = Resolver::new("2026-11-16").resolve(&q, &RegistrySource(&reg)).unwrap();
    assert_eq!(keys(&b)[0].0, "kc://policy/data-retention/customer-data-90-days@2.0.0");
}
