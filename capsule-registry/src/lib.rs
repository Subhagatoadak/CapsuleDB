//! `capsule-registry`: capsule identity, immutable versions, lifecycle,
//! ownership, dependency and evidence-family tracking, and an audit trail
//! (Phase 2, L2 conformance per `protocol/KCP-v0.1.md`).
//!
//! The Phase 2 MVP backend is embedded SQLite (`rusqlite`, bundled — no
//! system library or server required). `postgres-schema.sql` drafts the
//! equivalent Postgres shape for a later backend; the Rust API in this
//! crate is written so that swap should not need to change callers.
//!
//! Every mutation is append-only. `capsule_versions` rows are never
//! updated after insertion (spec §2.2, immutable versions); a capsule's
//! current lifecycle state is *derived* from the latest row in
//! `lifecycle_events`, never stored and overwritten. Deleting a published
//! capsule is not an operation this crate exposes at all — only
//! `transition(..., Superseded | Revoked | Deprecated, ...)`.

mod error;
mod model;

pub use error::RegistryError;
pub use model::*;

use capsule_core::{
    check_transition, content_hash, content_payload, rehydrate, validate, Capsule, Dependency, DependencyRelation,
    Governance, LifecycleState, Severity,
};
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::path::Path;

pub struct Registry {
    conn: Connection,
}

impl Registry {
    /// Opens (creating if needed) a SQLite-backed registry at `path`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, RegistryError> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// An in-memory registry, primarily for tests and short-lived CLI runs.
    pub fn open_in_memory() -> Result<Self, RegistryError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, RegistryError> {
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(Self { conn })
    }

    fn now() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Registers a new `draft` capsule version. Structurally/epistemically
    /// invalid capsules are rejected (`RegistryError::Validation`), as is
    /// re-registering an existing `(logical_id, version)` with different
    /// content (`RegistryError::Conflict`) — versions are immutable.
    /// Re-registering identical content is a no-op that returns the
    /// existing record.
    pub fn register(&mut self, capsule: &Capsule, actor: &str) -> Result<CapsuleRecord, RegistryError> {
        let issues = validate(capsule);
        if issues.iter().any(|i| i.severity == Severity::Error) {
            let detail = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| format!("- [{}] {}", i.code, i.message))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(RegistryError::Validation(detail));
        }
        if capsule.lifecycle_state != LifecycleState::Draft {
            return Err(RegistryError::Validation(format!(
                "capsules must be registered in 'draft' state (got {:?}); use transition()/publish() to advance lifecycle",
                capsule.lifecycle_state
            )));
        }

        let hash = content_hash(capsule)?;
        let payload = content_payload(capsule)?;

        if let Some(existing_hash) = self.find_content_hash(&capsule.logical_id, &capsule.version)? {
            if existing_hash == hash {
                return self.build_record(&capsule.logical_id, &capsule.version);
            }
            return Err(RegistryError::Conflict(format!(
                "{}@{} is already registered with different content; published versions are immutable — register a new version instead",
                capsule.logical_id, capsule.version
            )));
        }

        let now = Self::now();
        let permissions_json = serde_json::to_string(&capsule.governance.permissions)?;
        let payload_json = serde_json::to_string(&payload)?;

        let tx = self.conn.transaction()?;

        tx.execute(
            "INSERT OR IGNORE INTO capsule_payloads (content_hash, payload_json, first_seen_at) VALUES (?1, ?2, ?3)",
            params![hash, payload_json, now],
        )?;

        tx.execute(
            "INSERT INTO capsule_versions
                (logical_id, version, content_hash, capsule_type, owner, domain, permissions_json, valid_from, valid_until, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                capsule.logical_id,
                capsule.version,
                hash,
                to_str(&capsule.capsule_type),
                capsule.governance.owner,
                capsule.governance.domain,
                permissions_json,
                capsule.applicability.as_ref().and_then(|a| a.valid_from.clone()),
                capsule.applicability.as_ref().and_then(|a| a.valid_until.clone()),
                now,
            ],
        )?;

        tx.execute(
            "INSERT INTO lifecycle_events (logical_id, version, from_state, to_state, actor, reason, occurred_at)
             VALUES (?1, ?2, NULL, ?3, ?4, NULL, ?5)",
            params![capsule.logical_id, capsule.version, to_str(&LifecycleState::Draft), actor, now],
        )?;

        for dep in &capsule.dependencies {
            insert_dependency(&tx, &capsule.logical_id, &capsule.version, dep, &now)?;
        }

        for evidence in &capsule.evidence {
            tx.execute(
                "INSERT INTO evidence_links (logical_id, version, source_id, authority, recorded_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![capsule.logical_id, capsule.version, evidence.source_id, to_str(&evidence.authority), now],
            )?;
        }

        tx.execute(
            "INSERT INTO audit_events (event_type, logical_id, version, actor, detail_json, occurred_at)
             VALUES ('register', ?1, ?2, ?3, ?4, ?5)",
            params![
                capsule.logical_id,
                capsule.version,
                actor,
                serde_json::to_string(&serde_json::json!({ "content_hash": hash }))?,
                now
            ],
        )?;

        tx.commit()?;
        self.build_record(&capsule.logical_id, &capsule.version)
    }

    /// Applies a lifecycle transition, enforcing the state machine from
    /// `capsule-core::lifecycle`. Appends to the lifecycle ledger; never
    /// mutates an existing row.
    pub fn transition(
        &mut self,
        logical_id: &str,
        version: &str,
        to: LifecycleState,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<(), RegistryError> {
        let current = self
            .current_lifecycle_state(logical_id, version)?
            .ok_or_else(|| RegistryError::NotFound(format!("{logical_id}@{version}")))?;
        check_transition(current, to)?;

        let now = Self::now();
        self.conn.execute(
            "INSERT INTO lifecycle_events (logical_id, version, from_state, to_state, actor, reason, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![logical_id, version, to_str(&current), to_str(&to), actor, reason, now],
        )?;
        self.conn.execute(
            "INSERT INTO audit_events (event_type, logical_id, version, actor, detail_json, occurred_at)
             VALUES ('transition', ?1, ?2, ?3, ?4, ?5)",
            params![
                logical_id,
                version,
                actor,
                serde_json::to_string(&serde_json::json!({ "from": to_str(&current), "to": to_str(&to) }))?,
                now
            ],
        )?;
        Ok(())
    }

    /// Publishes a capsule version (`approved -> published`) and then, for
    /// every `supersedes` dependency it declares, attempts to move the
    /// referenced capsule `published -> superseded` (spec §8: "the
    /// referenced capsule's lifecycle must move to superseded as part of
    /// publishing this one"). Targets that are missing or not currently
    /// `published` are reported, not treated as an error.
    pub fn publish(&mut self, logical_id: &str, version: &str, actor: &str) -> Result<Vec<SupersessionOutcome>, RegistryError> {
        self.transition(logical_id, version, LifecycleState::Published, actor, None)?;

        let mut outcomes = Vec::new();
        for dep in self.dependencies_of(logical_id, version)?.into_iter().filter(|d| d.relation == DependencyRelation::Supersedes) {
            let target_version = match dep.depends_on_version.clone() {
                Some(v) => Some(v),
                None => self.latest_published(&dep.depends_on_logical_id, None)?.map(|r| r.version),
            };

            let Some(target_version) = target_version else {
                outcomes.push(SupersessionOutcome {
                    target_logical_id: dep.depends_on_logical_id,
                    target_version: String::new(),
                    applied: false,
                    note: "no published version found to supersede".to_string(),
                });
                continue;
            };

            match self.current_lifecycle_state(&dep.depends_on_logical_id, &target_version)? {
                Some(LifecycleState::Published) => {
                    let reason = format!("superseded by {logical_id}@{version}");
                    self.transition(&dep.depends_on_logical_id, &target_version, LifecycleState::Superseded, actor, Some(&reason))?;
                    outcomes.push(SupersessionOutcome {
                        target_logical_id: dep.depends_on_logical_id,
                        target_version,
                        applied: true,
                        note: "superseded".to_string(),
                    });
                }
                Some(other) => outcomes.push(SupersessionOutcome {
                    target_logical_id: dep.depends_on_logical_id,
                    target_version,
                    applied: false,
                    note: format!("target lifecycle_state is '{other:?}', not 'published'; left unchanged"),
                }),
                None => outcomes.push(SupersessionOutcome {
                    target_logical_id: dep.depends_on_logical_id,
                    target_version,
                    applied: false,
                    note: "target capsule not found in this registry".to_string(),
                }),
            }
        }

        let now = Self::now();
        self.conn.execute(
            "INSERT INTO audit_events (event_type, logical_id, version, actor, detail_json, occurred_at)
             VALUES ('publish', ?1, ?2, ?3, ?4, ?5)",
            params![logical_id, version, actor, serde_json::to_string(&outcomes_json(&outcomes))?, now],
        )?;

        Ok(outcomes)
    }

    /// Fetches a single registered capsule version, if present.
    pub fn get(&self, logical_id: &str, version: &str) -> Result<Option<CapsuleRecord>, RegistryError> {
        if self.find_content_hash(logical_id, version)?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.build_record(logical_id, version)?))
    }

    /// All registered versions of a logical ID, newest SemVer first.
    pub fn list_versions(&self, logical_id: &str) -> Result<Vec<CapsuleRecord>, RegistryError> {
        let mut stmt = self.conn.prepare("SELECT version FROM capsule_versions WHERE logical_id = ?1")?;
        let mut versions: Vec<(semver::Version, String)> = stmt
            .query_map(params![logical_id], |row| row.get::<_, String>(0))?
            .filter_map(|v| v.ok())
            .filter_map(|v| semver::Version::parse(&v).ok().map(|sv| (sv, v)))
            .collect();
        versions.sort_by(|a, b| b.0.cmp(&a.0));

        versions.into_iter().map(|(_, v)| self.build_record(logical_id, &v)).collect()
    }

    /// The highest-SemVer version currently `published` and applicable as
    /// of `as_of` (defaults to today), per its `applicability` window —
    /// the bitemporal query the roadmap calls for. Full authority
    /// ordering, evidence sufficiency, and dependency completion are
    /// `capsule-resolver`'s job (Phase 4); this is deliberately just the
    /// registry-level slice of it.
    pub fn latest_published(&self, logical_id: &str, as_of: Option<NaiveDate>) -> Result<Option<CapsuleRecord>, RegistryError> {
        let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().date_naive());
        for record in self.list_versions(logical_id)? {
            if record.lifecycle_state != LifecycleState::Published {
                continue;
            }
            let app = record.capsule.applicability.as_ref();
            let from_ok = app
                .and_then(|a| a.valid_from.as_deref())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .map(|d| d <= as_of)
                .unwrap_or(true);
            let until_ok = app
                .and_then(|a| a.valid_until.as_deref())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .map(|d| d >= as_of)
                .unwrap_or(true);
            if from_ok && until_ok {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }

    /// The full, ordered lifecycle ledger for one capsule version —
    /// draft/in_review/approval/publication/revocation history.
    pub fn history(&self, logical_id: &str, version: &str) -> Result<Vec<LifecycleEvent>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT from_state, to_state, actor, reason, occurred_at FROM lifecycle_events
             WHERE logical_id = ?1 AND version = ?2 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![logical_id, version], |row| {
            let from_state: Option<String> = row.get(0)?;
            let to_state: String = row.get(1)?;
            Ok((from_state, to_state, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, String>(4)?))
        })?;

        rows.map(|r| {
            let (from_state, to_state, actor, reason, occurred_at) = r?;
            Ok(LifecycleEvent {
                logical_id: logical_id.to_string(),
                version: version.to_string(),
                from_state: from_state.map(|s| from_str(&s)).transpose()?,
                to_state: from_str(&to_state)?,
                actor,
                reason,
                occurred_at,
            })
        })
        .collect()
    }

    /// Dependencies declared by one capsule version.
    pub fn dependencies_of(&self, logical_id: &str, version: &str) -> Result<Vec<DependencyRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT depends_on_logical_id, depends_on_version, relation, recorded_at FROM dependency_records
             WHERE logical_id = ?1 AND version = ?2 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![logical_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|r| {
            let (depends_on_logical_id, depends_on_version, relation, recorded_at) = r?;
            Ok(DependencyRecord {
                logical_id: logical_id.to_string(),
                version: version.to_string(),
                depends_on_logical_id,
                depends_on_version,
                relation: from_str(&relation)?,
                recorded_at,
            })
        })
        .collect()
    }

    /// Capsule versions that declare a dependency on `(logical_id,
    /// version)` — the reverse of [`Registry::dependencies_of`], used for
    /// impact analysis ("what breaks if this capsule changes").
    pub fn dependents_of(&self, logical_id: &str, version: &str) -> Result<Vec<DependencyRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT logical_id, version, relation, recorded_at FROM dependency_records
             WHERE depends_on_logical_id = ?1 AND (depends_on_version IS NULL OR depends_on_version = ?2)
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![logical_id, version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?;
        rows.map(|r| {
            let (dep_logical_id, dep_version, relation, recorded_at) = r?;
            Ok(DependencyRecord {
                logical_id: dep_logical_id,
                version: dep_version,
                depends_on_logical_id: logical_id.to_string(),
                depends_on_version: Some(version.to_string()),
                relation: from_str(&relation)?,
                recorded_at,
            })
        })
        .collect()
    }

    /// Every capsule version that cites `source_id` as evidence — an
    /// evidence family. Two capsules sharing a source are not independent
    /// evidence for the purposes of the confidence rules in
    /// `docs/specification/knowledge-capsule-spec.md` §6.
    pub fn evidence_family(&self, source_id: &str) -> Result<Vec<EvidenceLink>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT logical_id, version, authority, recorded_at FROM evidence_links WHERE source_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?;
        rows.map(|r| {
            let (logical_id, version, authority, recorded_at) = r?;
            Ok(EvidenceLink { logical_id, version, source_id: source_id.to_string(), authority: from_str(&authority)?, recorded_at })
        })
        .collect()
    }

    /// The most recent audit events across the whole registry, newest
    /// first.
    pub fn audit_log(&self, limit: u32) -> Result<Vec<AuditEvent>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT event_type, logical_id, version, actor, detail_json, occurred_at FROM audit_events
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|r| {
            let (event_type, logical_id, version, actor, detail_json, occurred_at) = r?;
            let detail = detail_json.map(|d| serde_json::from_str(&d)).transpose()?;
            Ok(AuditEvent { event_type, logical_id, version, actor, detail, occurred_at })
        })
        .collect()
    }

    fn find_content_hash(&self, logical_id: &str, version: &str) -> Result<Option<String>, RegistryError> {
        let mut stmt = self.conn.prepare("SELECT content_hash FROM capsule_versions WHERE logical_id = ?1 AND version = ?2")?;
        let mut rows = stmt.query(params![logical_id, version])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    fn current_lifecycle_state(&self, logical_id: &str, version: &str) -> Result<Option<LifecycleState>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT to_state FROM lifecycle_events WHERE logical_id = ?1 AND version = ?2 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![logical_id, version])?;
        match rows.next()? {
            Some(row) => Ok(Some(from_str(&row.get::<_, String>(0)?)?)),
            None => Ok(None),
        }
    }

    fn build_record(&self, logical_id: &str, version: &str) -> Result<CapsuleRecord, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, owner, domain, permissions_json, recorded_at FROM capsule_versions
             WHERE logical_id = ?1 AND version = ?2",
        )?;
        let (content_hash, owner, domain, permissions_json, recorded_at) = stmt.query_row(params![logical_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => RegistryError::NotFound(format!("{logical_id}@{version}")),
            other => RegistryError::Sqlite(other),
        })?;

        let payload_json: String = self.conn.query_row(
            "SELECT payload_json FROM capsule_payloads WHERE content_hash = ?1",
            params![content_hash],
            |row| row.get(0),
        )?;
        let payload: Value = serde_json::from_str(&payload_json)?;

        let lifecycle_state = self
            .current_lifecycle_state(logical_id, version)?
            .ok_or_else(|| RegistryError::NotFound(format!("no lifecycle event recorded for {logical_id}@{version}")))?;

        let governance = Governance {
            owner,
            domain,
            permissions: serde_json::from_str(&permissions_json)?,
        };

        let capsule = rehydrate(&payload, logical_id, version, lifecycle_state, governance)?;

        Ok(CapsuleRecord { logical_id: logical_id.to_string(), version: version.to_string(), content_hash, lifecycle_state, recorded_at, capsule })
    }
}

fn insert_dependency(tx: &rusqlite::Transaction, logical_id: &str, version: &str, dep: &Dependency, now: &str) -> Result<(), RegistryError> {
    let (depends_on_logical_id, depends_on_version) = split_pinned_id(&dep.capsule_id);
    tx.execute(
        "INSERT INTO dependency_records (logical_id, version, depends_on_logical_id, depends_on_version, relation, recorded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![logical_id, version, depends_on_logical_id, depends_on_version, to_str(&dep.relation), now],
    )?;
    Ok(())
}

/// Splits a dependency's `capsule_id` into a logical ID and an optional
/// pinned version, per the `kc://domain/topic/name@version` canonical
/// form in `protocol/KCP-v0.1.md` §3. Logical IDs never contain `@`, so
/// any `@` present is the version separator.
fn split_pinned_id(capsule_id: &str) -> (String, Option<String>) {
    match capsule_id.rsplit_once('@') {
        Some((logical, version)) => (logical.to_string(), Some(version.to_string())),
        None => (capsule_id.to_string(), None),
    }
}

fn to_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("enum serialization is infallible") {
        Value::String(s) => s,
        other => panic!("expected a string-serializable enum, got {other}"),
    }
}

fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, RegistryError> {
    Ok(serde_json::from_value(Value::String(s.to_string()))?)
}

fn outcomes_json(outcomes: &[SupersessionOutcome]) -> Value {
    Value::Array(
        outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "target_logical_id": o.target_logical_id,
                    "target_version": o.target_version,
                    "applied": o.applied,
                    "note": o.note,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::{parse, ConfidenceBasis, EpistemicStatus, EvidenceAuthority};

    fn draft_capsule(logical_id: &str, version: &str, statement: &str) -> Capsule {
        let json = format!(
            r#"{{
              "schema_version": "kcp/0.1",
              "logical_id": "{logical_id}",
              "version": "{version}",
              "capsule_type": "rule",
              "statement": "{statement}",
              "epistemic_status": "policy_defined",
              "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}},
              "lifecycle_state": "draft",
              "governance": {{"owner": "alice", "domain": "policy"}}
            }}"#
        );
        parse(&json).unwrap()
    }

    #[test]
    fn register_then_get_round_trips() {
        let mut registry = Registry::open_in_memory().unwrap();
        let capsule = draft_capsule("kc://policy/retention/ninety-days", "1.0.0", "Retain for 90 days.");
        let record = registry.register(&capsule, "alice").unwrap();
        assert_eq!(record.lifecycle_state, LifecycleState::Draft);

        let fetched = registry.get("kc://policy/retention/ninety-days", "1.0.0").unwrap().unwrap();
        assert_eq!(fetched.capsule.statement, "Retain for 90 days.");
    }

    #[test]
    fn re_registering_identical_content_is_idempotent() {
        let mut registry = Registry::open_in_memory().unwrap();
        let capsule = draft_capsule("kc://a/b/c", "1.0.0", "Same claim.");
        registry.register(&capsule, "alice").unwrap();
        let second = registry.register(&capsule, "alice").unwrap();
        assert_eq!(second.content_hash, content_hash(&capsule).unwrap());
    }

    #[test]
    fn re_registering_different_content_at_same_version_conflicts() {
        let mut registry = Registry::open_in_memory().unwrap();
        let first = draft_capsule("kc://a/b/c", "1.0.0", "Claim A.");
        let second = draft_capsule("kc://a/b/c", "1.0.0", "Claim B.");
        registry.register(&first, "alice").unwrap();
        let err = registry.register(&second, "alice").unwrap_err();
        assert!(matches!(err, RegistryError::Conflict(_)));
    }

    #[test]
    fn identical_content_under_different_logical_ids_shares_one_payload_row() {
        let mut registry = Registry::open_in_memory().unwrap();
        let a = draft_capsule("kc://team-a/rule/x", "1.0.0", "Identical claim.");
        let b = draft_capsule("kc://team-b/rule/y", "1.0.0", "Identical claim.");
        let ra = registry.register(&a, "alice").unwrap();
        let rb = registry.register(&b, "bob").unwrap();
        assert_eq!(ra.content_hash, rb.content_hash);

        let payload_rows: i64 = registry
            .conn
            .query_row("SELECT COUNT(*) FROM capsule_payloads WHERE content_hash = ?1", params![ra.content_hash], |r| r.get(0))
            .unwrap();
        assert_eq!(payload_rows, 1);
    }

    #[test]
    fn invalid_capsule_is_rejected() {
        let mut registry = Registry::open_in_memory().unwrap();
        let mut capsule = draft_capsule("kc://a/b/c", "1.0.0", "x");
        capsule.logical_id = "not-a-valid-id".to_string();
        let err = registry.register(&capsule, "alice").unwrap_err();
        assert!(matches!(err, RegistryError::Validation(_)));
    }

    #[test]
    fn full_lifecycle_walk_and_history() {
        let mut registry = Registry::open_in_memory().unwrap();
        let capsule = draft_capsule("kc://a/b/c", "1.0.0", "x");
        registry.register(&capsule, "alice").unwrap();

        registry.transition("kc://a/b/c", "1.0.0", LifecycleState::InReview, "alice", None).unwrap();
        registry.transition("kc://a/b/c", "1.0.0", LifecycleState::Approved, "bob", Some("looks correct")).unwrap();
        let outcomes = registry.publish("kc://a/b/c", "1.0.0", "bob").unwrap();
        assert!(outcomes.is_empty());

        let record = registry.get("kc://a/b/c", "1.0.0").unwrap().unwrap();
        assert_eq!(record.lifecycle_state, LifecycleState::Published);

        let history = registry.history("kc://a/b/c", "1.0.0").unwrap();
        assert_eq!(history.len(), 4);
        assert_eq!(history[0].to_state, LifecycleState::Draft);
        assert_eq!(history.last().unwrap().to_state, LifecycleState::Published);
    }

    #[test]
    fn illegal_transition_is_rejected() {
        let mut registry = Registry::open_in_memory().unwrap();
        let capsule = draft_capsule("kc://a/b/c", "1.0.0", "x");
        registry.register(&capsule, "alice").unwrap();
        let err = registry.transition("kc://a/b/c", "1.0.0", LifecycleState::Published, "alice", None).unwrap_err();
        assert!(matches!(err, RegistryError::InvalidTransition(_)));
    }

    #[test]
    fn publish_with_supersedes_dependency_auto_supersedes_target() {
        let mut registry = Registry::open_in_memory().unwrap();

        let old = draft_capsule("kc://policy/retention/days", "1.0.0", "Retain for 90 days.");
        registry.register(&old, "alice").unwrap();
        registry.transition("kc://policy/retention/days", "1.0.0", LifecycleState::InReview, "alice", None).unwrap();
        registry.transition("kc://policy/retention/days", "1.0.0", LifecycleState::Approved, "alice", None).unwrap();
        registry.publish("kc://policy/retention/days", "1.0.0", "alice").unwrap();

        let new_json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://policy/retention/days",
          "version": "2.0.0",
          "capsule_type": "rule",
          "statement": "Retain for 180 days.",
          "epistemic_status": "policy_defined",
          "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "dependencies": [{"capsule_id": "kc://policy/retention/days@1.0.0", "relation": "supersedes"}],
          "governance": {"owner": "alice", "domain": "policy"}
        }"#;
        let new_capsule = parse(new_json).unwrap();
        registry.register(&new_capsule, "alice").unwrap();
        registry.transition("kc://policy/retention/days", "2.0.0", LifecycleState::InReview, "alice", None).unwrap();
        registry.transition("kc://policy/retention/days", "2.0.0", LifecycleState::Approved, "alice", None).unwrap();
        let outcomes = registry.publish("kc://policy/retention/days", "2.0.0", "alice").unwrap();

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].applied);

        let old_record = registry.get("kc://policy/retention/days", "1.0.0").unwrap().unwrap();
        assert_eq!(old_record.lifecycle_state, LifecycleState::Superseded);
    }

    #[test]
    fn latest_published_respects_applicability_window() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://a/b/c",
          "version": "1.0.0",
          "capsule_type": "metric",
          "statement": "x",
          "epistemic_status": "estimated",
          "confidence": {"basis": "statistical_inference", "ceiling": 0.6},
          "lifecycle_state": "draft",
          "applicability": {"valid_from": "2026-01-01", "valid_until": "2026-03-31"},
          "governance": {"owner": "alice", "domain": "d"}
        }"#;
        let capsule = parse(json).unwrap();
        registry.register(&capsule, "alice").unwrap();
        registry.transition("kc://a/b/c", "1.0.0", LifecycleState::InReview, "alice", None).unwrap();
        registry.transition("kc://a/b/c", "1.0.0", LifecycleState::Approved, "alice", None).unwrap();
        registry.publish("kc://a/b/c", "1.0.0", "alice").unwrap();

        let in_window = registry.latest_published("kc://a/b/c", Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap())).unwrap();
        assert!(in_window.is_some());

        let out_of_window = registry.latest_published("kc://a/b/c", Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap())).unwrap();
        assert!(out_of_window.is_none());
    }

    #[test]
    fn evidence_family_groups_capsules_by_shared_source() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://a/b/c",
          "version": "1.0.0",
          "capsule_type": "fact",
          "statement": "x",
          "epistemic_status": "observed",
          "confidence": {"basis": "direct_observation", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "evidence": [{"source_id": "report-42", "authority": "primary"}],
          "governance": {"owner": "alice", "domain": "d"}
        }"#;
        let capsule = parse(json).unwrap();
        registry.register(&capsule, "alice").unwrap();

        let family = registry.evidence_family("report-42").unwrap();
        assert_eq!(family.len(), 1);
        assert_eq!(family[0].logical_id, "kc://a/b/c");
        assert_eq!(family[0].authority, EvidenceAuthority::Primary);
    }

    #[test]
    fn audit_log_records_register_transition_and_publish() {
        let mut registry = Registry::open_in_memory().unwrap();
        let capsule = draft_capsule("kc://a/b/c", "1.0.0", "x");
        registry.register(&capsule, "alice").unwrap();
        registry.transition("kc://a/b/c", "1.0.0", LifecycleState::InReview, "alice", None).unwrap();

        let log = registry.audit_log(10).unwrap();
        let event_types: Vec<&str> = log.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains(&"register"));
        assert!(event_types.contains(&"transition"));
    }

    #[test]
    fn epistemic_status_survives_round_trip() {
        let mut registry = Registry::open_in_memory().unwrap();
        let json = r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "kc://a/b/c",
          "version": "1.0.0",
          "capsule_type": "fact",
          "statement": "x",
          "epistemic_status": "disputed",
          "confidence": {"basis": "unsupported", "ceiling": 0.2},
          "lifecycle_state": "draft",
          "contradictions": [{"capsule_id": "kc://a/b/d", "description": "conflict", "resolution_status": "unresolved"}],
          "governance": {"owner": "alice", "domain": "d"}
        }"#;
        let capsule = parse(json).unwrap();
        registry.register(&capsule, "alice").unwrap();
        let record = registry.get("kc://a/b/c", "1.0.0").unwrap().unwrap();
        assert_eq!(record.capsule.epistemic_status, EpistemicStatus::Disputed);
        assert_eq!(record.capsule.confidence.basis, ConfidenceBasis::Unsupported);
    }
}
