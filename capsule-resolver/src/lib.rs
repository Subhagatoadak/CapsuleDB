//! `capsule-resolver`: resolves the smallest applicable, authoritative
//! capsule set for a request (Phase 4), rather than merely returning
//! semantically similar text.
//!
//! ```rust,no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = capsule_registry::Registry::open_in_memory()?;
//! let resolver = capsule_resolver::Resolver::new(&registry);
//! let request = capsule_resolver::ResolutionRequest::new("privacy review");
//! let bundle = resolver.resolve(&request)?;
//! # let _ = bundle;
//! # Ok(())
//! # }
//! ```
//!
//! Not implemented: an optional semantic (embedding) discovery index —
//! `about` matching is a literal substring scan over statements, which is
//! the "lexical discovery index" the roadmap asks for and no more.

mod error;
mod resolver;
mod types;

pub use error::ResolverError;
pub use resolver::Resolver;
pub use types::{
    ContradictionBundleEntry, Decision, IncludedAs, KnownGap, PlanStep, ResolutionBundle, ResolutionRequest, ResolvedCapsule,
};

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, LifecycleState};
    use capsule_registry::Registry;

    fn publish(registry: &mut Registry, json: &str) -> (String, String) {
        let capsule = parse(json).unwrap();
        let record = registry.register(&capsule, "alice").unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::InReview, "alice", None).unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::Approved, "alice", None).unwrap();
        registry.publish(&record.logical_id, &record.version, "alice").unwrap();
        (record.logical_id, record.version)
    }

    fn capsule_json(logical_id: &str, statement: &str, extra: &str) -> String {
        format!(
            r#"{{
              "schema_version": "kcp/0.1",
              "logical_id": "{logical_id}",
              "version": "1.0.0",
              "capsule_type": "rule",
              "statement": "{statement}",
              "epistemic_status": "policy_defined",
              "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}},
              "lifecycle_state": "draft",
              "governance": {{"owner": "alice", "domain": "policy"}}
              {extra}
            }}"#
        )
    }

    #[test]
    fn resolves_by_domain_and_keyword() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(&mut registry, &capsule_json("kc://policy/retention/rule-a", "Retain data for 90 days.", ""));
        publish(&mut registry, &capsule_json("kc://marketing/lift/rule-b", "Retain nothing special here.", ""));

        let resolver = Resolver::new(&registry);
        let mut request = ResolutionRequest::new("privacy review");
        request.domain = Some("policy".to_string());
        request.about = Some("retain".to_string());

        let bundle = resolver.resolve(&request).unwrap();
        assert_eq!(bundle.capsules.len(), 1);
        assert_eq!(bundle.capsules[0].logical_id, "kc://policy/retention/rule-a");
    }

    #[test]
    fn excludes_unpermitted_consumer_role() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/internal/rule-a",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Internal only.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "governance": {"owner": "alice", "domain": "policy", "permissions": ["legal-team"]}
        }"#;
        publish(&mut registry, json);

        let resolver = Resolver::new(&registry);
        let mut request = ResolutionRequest::new("check");
        request.consumer_role = Some("marketing".to_string());
        let bundle = resolver.resolve(&request).unwrap();
        assert!(bundle.capsules.is_empty());
        assert!(bundle.plan.iter().any(|s| s.reason.contains("consumer role")));

        request.consumer_role = Some("legal-team".to_string());
        let bundle = resolver.resolve(&request).unwrap();
        assert_eq!(bundle.capsules.len(), 1);
    }

    #[test]
    fn anonymous_request_does_not_see_restricted_capsules() {
        // Regression: a request with no consumer_role at all must be
        // treated as unprivileged, not as bypassing permission checks.
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/internal/rule-a",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Internal only.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "governance": {"owner": "alice", "domain": "policy", "permissions": ["legal-team"]}
        }"#;
        publish(&mut registry, json);

        let resolver = Resolver::new(&registry);
        let request = ResolutionRequest::new("check");
        assert!(request.consumer_role.is_none());
        let bundle = resolver.resolve(&request).unwrap();
        assert!(bundle.capsules.is_empty());
    }

    #[test]
    fn dependency_completion_propagates_confidence_ceiling() {
        let mut registry = Registry::open_in_memory().unwrap();
        let base_json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/base/rule",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Base rule with weak support.",
          "epistemic_status": "disputed",
          "confidence": {"basis": "unsupported", "ceiling": 0.2},
          "lifecycle_state": "draft",
          "contradictions": [{"capsule_id": "kc://policy/other/rule", "description": "conflict", "resolution_status": "unresolved"}],
          "governance": {"owner": "alice", "domain": "policy"}
        }"#;
        publish(&mut registry, base_json);

        let dependent_json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/dependent/rule",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Dependent rule.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "dependencies": [{"capsule_id": "kc://policy/base/rule@1.0.0", "relation": "requires"}],
          "governance": {"owner": "alice", "domain": "policy"}
        }"#;
        publish(&mut registry, dependent_json);

        let resolver = Resolver::new(&registry);
        let mut request = ResolutionRequest::new("check");
        request.domain = Some("policy".to_string());
        request.about = Some("dependent".to_string());
        let bundle = resolver.resolve(&request).unwrap();

        let dependent = bundle.capsules.iter().find(|c| c.logical_id == "kc://policy/dependent/rule").unwrap();
        assert_eq!(dependent.effective_confidence_ceiling, 0.2, "ceiling must be capped by the unsupported dependency");

        let base = bundle.capsules.iter().find(|c| c.logical_id == "kc://policy/base/rule").unwrap();
        assert_eq!(base.included_as, IncludedAs::Dependency);

        // the base capsule's own unresolved contradiction should surface
        assert!(!bundle.contradictions.is_empty());
    }

    #[test]
    fn missing_required_dependency_is_a_known_gap() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/dependent/rule",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Depends on something missing.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "dependencies": [{"capsule_id": "kc://policy/missing/rule@1.0.0", "relation": "requires"}],
          "governance": {"owner": "alice", "domain": "policy"}
        }"#;
        publish(&mut registry, json);

        let resolver = Resolver::new(&registry);
        let request = ResolutionRequest::new("check");
        let bundle = resolver.resolve(&request).unwrap();
        assert!(bundle.gaps.iter().any(|g| g.logical_id == "kc://policy/missing/rule"));
    }

    #[test]
    fn replay_reruns_the_stored_request() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(&mut registry, &capsule_json("kc://policy/retention/rule-a", "Retain data.", ""));

        let resolver = Resolver::new(&registry);
        let bundle = resolver.resolve(&ResolutionRequest::new("check")).unwrap();
        let replayed = resolver.replay(&bundle).unwrap();
        assert_eq!(bundle.capsules.len(), replayed.capsules.len());
    }

    #[test]
    fn evidence_sufficiency_gate_excludes_unverified_only_evidence() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/weak/rule",
          "version": "1.0.0",
          "capsule_type": "fact",
          "statement": "Weakly evidenced claim.",
          "epistemic_status": "estimated",
          "confidence": {"basis": "heuristic", "ceiling": 0.4},
          "lifecycle_state": "draft",
          "evidence": [{"source_id": "s1", "authority": "unverified"}],
          "governance": {"owner": "alice", "domain": "policy"}
        }"#;
        publish(&mut registry, json);

        let resolver = Resolver::new(&registry);
        let mut request = ResolutionRequest::new("check");
        request.require_sufficient_evidence = true;
        let bundle = resolver.resolve(&request).unwrap();
        assert!(bundle.capsules.is_empty());
    }
}
