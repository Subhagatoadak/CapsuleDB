//! A local cache of registered capsules, plus revocation-propagation
//! support: invalidating cached entries as they change in the registry.
//!
//! This is the "offline mode" story too: once a capsule is cached, `Cache::get`
//! can serve it with no registry access at all. There's no persistence —
//! it's a process-lifetime in-memory cache, not a disk-backed offline
//! store.

use capsule_registry::CapsuleRecord;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct Cache {
    entries: RwLock<HashMap<(String, String), CapsuleRecord>>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, logical_id: &str, version: &str) -> Option<CapsuleRecord> {
        self.entries.read().unwrap().get(&(logical_id.to_string(), version.to_string())).cloned()
    }

    pub fn put(&self, record: CapsuleRecord) {
        let key = (record.logical_id.clone(), record.version.clone());
        self.entries.write().unwrap().insert(key, record);
    }

    /// Drops a specific cached version — used when a lifecycle transition
    /// (e.g. revoke, supersede) makes a cached copy stale.
    pub fn invalidate(&self, logical_id: &str, version: &str) {
        self.entries.write().unwrap().remove(&(logical_id.to_string(), version.to_string()));
    }

    pub fn invalidate_all(&self) {
        self.entries.write().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::*;

    fn record(logical_id: &str, version: &str) -> CapsuleRecord {
        CapsuleRecord {
            logical_id: logical_id.to_string(),
            version: version.to_string(),
            content_hash: "sha256:x".to_string(),
            lifecycle_state: LifecycleState::Published,
            recorded_at: "now".to_string(),
            capsule: Capsule {
                schema_version: SCHEMA_VERSION.to_string(),
                logical_id: logical_id.to_string(),
                version: version.to_string(),
                content_hash: None,
                capsule_type: CapsuleType::Fact,
                statement: "x".to_string(),
                epistemic_status: EpistemicStatus::Observed,
                confidence: Confidence { basis: ConfidenceBasis::DirectObservation, ceiling: 0.9, rationale: None },
                lifecycle_state: LifecycleState::Published,
                evidence: vec![],
                applicability: None,
                contradictions: vec![],
                dependencies: vec![],
                governance: Governance { owner: "a".to_string(), domain: "d".to_string(), permissions: vec![] },
                evaluation_contract: None,
            },
        }
    }

    #[test]
    fn put_then_get_round_trips() {
        let cache = Cache::new();
        cache.put(record("kc://a/b/c", "1.0.0"));
        assert!(cache.get("kc://a/b/c", "1.0.0").is_some());
        assert!(cache.get("kc://a/b/c", "2.0.0").is_none());
    }

    #[test]
    fn invalidate_removes_one_entry() {
        let cache = Cache::new();
        cache.put(record("kc://a/b/c", "1.0.0"));
        cache.put(record("kc://a/b/c", "2.0.0"));
        cache.invalidate("kc://a/b/c", "1.0.0");
        assert!(cache.get("kc://a/b/c", "1.0.0").is_none());
        assert!(cache.get("kc://a/b/c", "2.0.0").is_some());
        assert_eq!(cache.len(), 1);
    }
}
