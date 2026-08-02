//! `capsule-runtime`: the embeddable local runtime (Phase 5) — serves
//! capsules from a `capsule-registry`, enforcing policy, applying
//! consumer-specific views, caching, and recording usage, with signed
//! capsule verification available where a publisher's key is known.
//!
//! ```rust,no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = capsule_registry::Registry::open("capsules.sqlite3")?;
//! let runtime = capsule_runtime::Runtime::new(registry);
//! let view = runtime.get("kc://policy/x/y", "1.0.0", &capsule_runtime::AccessRequest::public())?;
//! # let _ = view;
//! # Ok(())
//! # }
//! ```
//!
//! What's here vs. what isn't: this crate is the **local embedded
//! runtime** and the primitives a **remote API runtime** builds on
//! (`capsule-api` is that remote runtime, using `capsule-registry` +
//! `capsule-resolver` underneath directly rather than importing this
//! crate — the split is by deployment mode, not by feature). Offline
//! mode is trivial by construction (SQLite is local; there's no server
//! to lose contact with) rather than a distinct code path. "Sync with a
//! central registry" is not implemented — there's no multi-registry
//! replication here, only single-registry polling for local cache
//! invalidation.

pub mod cache;
pub mod hex;
pub mod signing;
pub mod telemetry;
pub mod view;

pub use cache::Cache;
pub use signing::{verify_content_hash, KeyPair, SigningError};
pub use telemetry::{Telemetry, UsageEvent};
pub use view::{check_access, AccessDenied, AccessRequest, CapsuleView};

use capsule_registry::{AuditEvent, Registry, RegistryError};
use capsule_resolver::{ResolutionBundle, ResolutionRequest, Resolver, ResolverError};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Resolver(#[from] ResolverError),
    #[error(transparent)]
    AccessDenied(#[from] AccessDenied),
}

pub struct Runtime {
    registry: Registry,
    cache: Cache,
    telemetry: Telemetry,
    last_seen_audit_at: Mutex<String>,
}

impl Runtime {
    pub fn new(registry: Registry) -> Self {
        Self { registry, cache: Cache::new(), telemetry: Telemetry::new(), last_seen_audit_at: Mutex::new(String::new()) }
    }

    /// Fetches a capsule, applying `access`'s policy check and view
    /// redaction, and populating the local cache on a miss.
    pub fn get(&self, logical_id: &str, version: &str, access: &AccessRequest) -> Result<Option<CapsuleView>, RuntimeError> {
        let record = match self.cache.get(logical_id, version) {
            Some(cached) => Some(cached),
            None => match self.registry.get(logical_id, version)? {
                Some(record) => {
                    self.cache.put(record.clone());
                    Some(record)
                }
                None => None,
            },
        };

        let Some(record) = record else { return Ok(None) };
        check_access(&record.capsule, access)?;

        self.telemetry.record(UsageEvent {
            operation: "get",
            logical_id: logical_id.to_string(),
            version: Some(version.to_string()),
            consumer_role: access.consumer_role.clone(),
        });

        Ok(Some(view::build_view(record.capsule, access)))
    }

    /// Resolves a request, folding `access.consumer_role` into the
    /// request's permission filter if the caller didn't already set one.
    pub fn resolve(&self, request: &ResolutionRequest, access: &AccessRequest) -> Result<ResolutionBundle, RuntimeError> {
        let mut request = request.clone();
        if request.consumer_role.is_none() {
            request.consumer_role = access.consumer_role.clone();
        }
        let bundle = Resolver::new(&self.registry).resolve(&request)?;

        self.telemetry.record(UsageEvent {
            operation: "resolve",
            logical_id: format!("purpose:{}", request.purpose),
            version: None,
            consumer_role: access.consumer_role.clone(),
        });

        Ok(bundle)
    }

    /// Verifies a hex Ed25519 signature over a registered capsule's
    /// content hash.
    pub fn verify_signature(&self, logical_id: &str, version: &str, public_key_hex: &str, signature_hex: &str) -> Result<bool, RuntimeError> {
        let Some(record) = self.registry.get(logical_id, version)? else { return Ok(false) };
        Ok(verify_content_hash(public_key_hex, &record.content_hash, signature_hex).is_ok())
    }

    /// Polls the registry's audit log for events since the last poll,
    /// invalidating any cached capsule a lifecycle transition touched
    /// ("revocation propagation" / the local half of "update
    /// subscriptions" — there is no push mechanism, only pull).
    pub fn poll_updates(&self) -> Result<Vec<AuditEvent>, RuntimeError> {
        let mut last_seen = self.last_seen_audit_at.lock().unwrap();
        let recent = self.registry.audit_log(500)?; // newest first
        let mut new_events: Vec<AuditEvent> = recent.into_iter().filter(|e| e.occurred_at.as_str() > last_seen.as_str()).collect();

        if let Some(latest) = new_events.iter().map(|e| e.occurred_at.clone()).max() {
            *last_seen = latest;
        }
        new_events.reverse(); // oldest first

        for event in &new_events {
            if event.event_type == "transition" {
                if let (Some(logical_id), Some(version)) = (&event.logical_id, &event.version) {
                    self.cache.invalidate(logical_id, version);
                }
            }
        }

        Ok(new_events)
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    pub fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Always `true`: a local SQLite-backed runtime has no server to
    /// lose contact with. Documented as an explicit contract rather than
    /// left implicit.
    pub fn is_offline_capable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, LifecycleState};

    fn publish(registry: &mut Registry, json: &str) -> (String, String) {
        let capsule = parse(json).unwrap();
        let record = registry.register(&capsule, "alice").unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::InReview, "alice", None).unwrap();
        registry.transition(&record.logical_id, &record.version, LifecycleState::Approved, "alice", None).unwrap();
        registry.publish(&record.logical_id, &record.version, "alice").unwrap();
        (record.logical_id, record.version)
    }

    fn restricted_json() -> &'static str {
        r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/internal/rule",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Internal only.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "evidence": [{"source_id": "s1", "excerpt": "top secret text", "authority": "primary"}],
          "governance": {"owner": "alice", "domain": "policy", "permissions": ["legal-team"]}
        }"#
    }

    #[test]
    fn get_populates_cache_on_miss() {
        let mut registry = Registry::open_in_memory().unwrap();
        let (logical_id, version) = publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
              "capsule_type": "fact", "statement": "x", "epistemic_status": "policy_defined",
              "confidence": {"basis": "policy_mandate", "ceiling": 0.9}, "lifecycle_state": "draft",
              "governance": {"owner": "alice", "domain": "d"}
            }"#,
        );

        let runtime = Runtime::new(registry);
        assert_eq!(runtime.cache().len(), 0);
        let view = runtime.get(&logical_id, &version, &AccessRequest::public()).unwrap();
        assert!(view.is_some());
        assert_eq!(runtime.cache().len(), 1);
    }

    #[test]
    fn get_denies_access_without_matching_role() {
        let mut registry = Registry::open_in_memory().unwrap();
        let (logical_id, version) = publish(&mut registry, restricted_json());
        let runtime = Runtime::new(registry);

        let err = runtime.get(&logical_id, &version, &AccessRequest::public()).unwrap_err();
        assert!(matches!(err, RuntimeError::AccessDenied(_)));

        let access = AccessRequest { consumer_role: Some("legal-team".to_string()), ..Default::default() };
        assert!(runtime.get(&logical_id, &version, &access).unwrap().is_some());
    }

    #[test]
    fn get_redacts_evidence_without_read_permission() {
        let mut registry = Registry::open_in_memory().unwrap();
        let (logical_id, version) = publish(&mut registry, restricted_json());
        let runtime = Runtime::new(registry);

        let access = AccessRequest { consumer_role: Some("legal-team".to_string()), can_read_evidence: false, ..Default::default() };
        let view = runtime.get(&logical_id, &version, &access).unwrap().unwrap();
        assert_eq!(view.capsule.evidence[0].excerpt.as_deref(), Some("[redacted]"));
    }

    #[test]
    fn resolve_folds_access_role_into_request() {
        let mut registry = Registry::open_in_memory().unwrap();
        publish(&mut registry, restricted_json());
        let runtime = Runtime::new(registry);

        let request = capsule_resolver::ResolutionRequest::new("check");
        let bundle = runtime.resolve(&request, &AccessRequest::public()).unwrap();
        assert!(bundle.capsules.is_empty());

        let access = AccessRequest { consumer_role: Some("legal-team".to_string()), ..Default::default() };
        let bundle = runtime.resolve(&request, &access).unwrap();
        assert_eq!(bundle.capsules.len(), 1);
    }

    #[test]
    fn poll_updates_invalidates_cache_on_transition() {
        let mut registry = Registry::open_in_memory().unwrap();
        let (logical_id, version) = publish(
            &mut registry,
            r#"{
              "schema_version": "kcp/0.1", "logical_id": "kc://a/b/c", "version": "1.0.0",
              "capsule_type": "fact", "statement": "x", "epistemic_status": "policy_defined",
              "confidence": {"basis": "policy_mandate", "ceiling": 0.9}, "lifecycle_state": "draft",
              "governance": {"owner": "alice", "domain": "d"}
            }"#,
        );

        let mut runtime = Runtime::new(registry);
        runtime.get(&logical_id, &version, &AccessRequest::public()).unwrap();
        assert_eq!(runtime.cache().len(), 1);

        let events_before = runtime.poll_updates().unwrap();
        assert!(!events_before.is_empty());

        runtime.registry_mut().transition(&logical_id, &version, LifecycleState::Deprecated, "alice", None).unwrap();
        let events = runtime.poll_updates().unwrap();
        assert!(events.iter().any(|e| e.event_type == "transition"));
        assert_eq!(runtime.cache().len(), 0, "cache should be invalidated after a transition event");
    }
}
