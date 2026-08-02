//! Tenant isolation (Phase 10): one SQLite database file per tenant,
//! rather than a shared schema with a `tenant_id` column.
//!
//! This crate has five other crates built directly on `Registry`'s
//! query surface; retrofitting every query with a `WHERE tenant_id = ?`
//! is a lot of surface area to get right, and a missed clause anywhere
//! is a cross-tenant data leak. A file per tenant makes that class of
//! bug structurally impossible — there is no query that could read
//! another tenant's rows, because there is no connection open to
//! another tenant's file. The cost is real (no cross-tenant SQL joins,
//! N open files instead of one), and is judged worth it here.

use crate::{Registry, RegistryError};
use std::path::PathBuf;

/// Opens an isolated [`Registry`] per tenant under a shared base
/// directory. Tenant IDs are sanitized to `[a-zA-Z0-9_-]` before being
/// used as a filename, so a caller passing an untrusted tenant
/// identifier (e.g. from an API request) can't path-traverse out of
/// `base_dir`.
pub struct TenantStore {
    base_dir: PathBuf,
}

impl TenantStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Opens (creating if needed) the registry for `tenant_id`.
    pub fn registry_for(&self, tenant_id: &str) -> Result<Registry, RegistryError> {
        Registry::open(self.db_path(tenant_id))
    }

    /// The on-disk path a tenant's database lives at, without opening it.
    pub fn db_path(&self, tenant_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.sqlite3", sanitize(tenant_id)))
    }

    /// Every tenant with a database file under `base_dir` — not every
    /// tenant that has ever existed, just those with data on disk.
    pub fn known_tenants(&self) -> std::io::Result<Vec<String>> {
        let mut tenants = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if let Some(name) = entry.path().file_stem().and_then(|s| s.to_str()) {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("sqlite3") {
                    tenants.push(name.to_string());
                }
            }
        }
        tenants.sort();
        Ok(tenants)
    }
}

fn sanitize(tenant_id: &str) -> String {
    let cleaned: String = tenant_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, LifecycleState};

    fn tempdir() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("capsule-registry-tenant-test-{}-{nanos}", std::process::id()))
    }

    fn draft(logical_id: &str, statement: &str) -> capsule_core::Capsule {
        let json = format!(
            r#"{{
              "schema_version": "kcp/0.1", "logical_id": "{logical_id}", "version": "1.0.0",
              "capsule_type": "rule", "statement": "{statement}", "epistemic_status": "policy_defined",
              "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}}, "lifecycle_state": "draft",
              "governance": {{"owner": "alice", "domain": "policy"}}
            }}"#
        );
        parse(&json).unwrap()
    }

    #[test]
    fn tenants_are_fully_isolated() {
        let dir = tempdir();
        let store = TenantStore::new(&dir).unwrap();

        let mut acme = store.registry_for("acme-corp").unwrap();
        acme.register(&draft("kc://a/b/c", "Acme's rule."), "alice").unwrap();

        let mut globex = store.registry_for("globex").unwrap();
        globex.register(&draft("kc://a/b/c", "Globex's different rule."), "bob").unwrap();

        // Same logical_id, different content, different tenants: neither
        // registry sees the other's version, and no conflict is raised
        // (which would happen if they shared one underlying table).
        let acme_record = acme.get("kc://a/b/c", "1.0.0").unwrap().unwrap();
        let globex_record = globex.get("kc://a/b/c", "1.0.0").unwrap().unwrap();
        assert_eq!(acme_record.capsule.statement, "Acme's rule.");
        assert_eq!(globex_record.capsule.statement, "Globex's different rule.");
        assert_ne!(acme_record.content_hash, globex_record.content_hash);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reopening_a_tenant_returns_the_same_data() {
        let dir = tempdir();
        let store = TenantStore::new(&dir).unwrap();

        {
            let mut registry = store.registry_for("acme-corp").unwrap();
            let capsule = draft("kc://a/b/c", "Persisted rule.");
            let record = registry.register(&capsule, "alice").unwrap();
            registry.transition(&record.logical_id, &record.version, LifecycleState::InReview, "alice", None).unwrap();
        }

        let registry = store.registry_for("acme-corp").unwrap();
        let record = registry.get("kc://a/b/c", "1.0.0").unwrap().unwrap();
        assert_eq!(record.lifecycle_state, LifecycleState::InReview);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tenant_ids_are_sanitized_against_path_traversal() {
        let dir = tempdir();
        let store = TenantStore::new(&dir).unwrap();
        let path = store.db_path("../../etc/passwd");
        assert!(path.starts_with(&dir), "sanitized path must stay under base_dir, got {path:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn known_tenants_lists_databases_created_so_far() {
        let dir = tempdir();
        let store = TenantStore::new(&dir).unwrap();
        store.registry_for("acme-corp").unwrap();
        store.registry_for("globex").unwrap();

        let tenants = store.known_tenants().unwrap();
        assert_eq!(tenants, vec!["acme-corp".to_string(), "globex".to_string()]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
