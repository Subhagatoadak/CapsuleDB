//! Canonical serialization and content-addressed hashing (spec §2.3).
//!
//! The content hash addresses the **epistemic payload** of a capsule, not
//! the whole document. Identity and administrative fields —
//! `logical_id`, `version`, `lifecycle_state`, `governance`, and
//! `content_hash` itself — are excluded, so two capsules that assert the
//! same knowledge under different names, owners, or lifecycle states hash
//! identically. That equality is what `capsule-registry` uses for
//! content-addressed deduplication and for surfacing evidence reused
//! verbatim across unrelated capsules.

use crate::types::{Capsule, Governance, LifecycleState};
use crate::CapsuleError;
use serde_json::Value;
use sha2::{Digest, Sha256};

const IDENTITY_FIELDS: [&str; 5] = ["content_hash", "logical_id", "version", "lifecycle_state", "governance"];

/// Returns the epistemic payload of a capsule as a `serde_json::Value`:
/// the canonical document with identity/administrative fields removed.
/// This is what gets hashed, and what `capsule-registry` stores once per
/// distinct payload to deduplicate identical knowledge across capsules.
///
/// Relies on `serde_json::Map` being backed by a `BTreeMap` (the default,
/// i.e. the `preserve_order` feature is *not* enabled anywhere in the
/// dependency graph), which sorts keys by byte order during serialization,
/// giving a stable canonical form with no extra bookkeeping.
pub fn content_payload(capsule: &Capsule) -> serde_json::Result<Value> {
    let mut value = serde_json::to_value(capsule)?;
    if let Value::Object(map) = &mut value {
        for field in IDENTITY_FIELDS {
            map.remove(field);
        }
    }
    Ok(value)
}

/// Serializes a capsule's content payload to its canonical JSON string form
/// (UTF-8, sorted keys, no insignificant whitespace). This is the exact
/// byte sequence that gets hashed by [`content_hash`].
pub fn canonicalize(capsule: &Capsule) -> serde_json::Result<String> {
    serde_json::to_string(&content_payload(capsule)?)
}

/// Computes the `sha256:<hex>` content hash of a capsule's epistemic
/// payload (spec §2.3).
pub fn content_hash(capsule: &Capsule) -> serde_json::Result<String> {
    let canonical = canonicalize(capsule)?;
    Ok(hash_bytes(canonical.as_bytes()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Reconstructs a full capsule document from a stored content payload plus
/// the identity/administrative fields a registry tracks separately. The
/// inverse of [`content_payload`]; `content_hash(&rehydrate(&content_payload(&c)?, ...)?)`
/// reproduces the same hash the payload was stored under.
pub fn rehydrate(
    payload: &Value,
    logical_id: &str,
    version: &str,
    lifecycle_state: LifecycleState,
    governance: Governance,
) -> Result<Capsule, CapsuleError> {
    let mut value = payload.clone();
    let hash = hash_bytes(serde_json::to_string(&value)?.as_bytes());
    if let Value::Object(map) = &mut value {
        map.insert("logical_id".to_string(), Value::String(logical_id.to_string()));
        map.insert("version".to_string(), Value::String(version.to_string()));
        map.insert("lifecycle_state".to_string(), serde_json::to_value(lifecycle_state)?);
        map.insert("governance".to_string(), serde_json::to_value(governance)?);
        map.insert("content_hash".to_string(), Value::String(hash));
    }
    Ok(serde_json::from_value(value)?)
}

/// Fingerprints arbitrary source material (e.g. an evidence excerpt or a
/// whole source document) so provenance can be checked without re-storing
/// the source itself.
pub fn source_fingerprint(source_bytes: &[u8]) -> String {
    hash_bytes(source_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn minimal_capsule() -> Capsule {
        Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            logical_id: "kc://domain/topic/name".to_string(),
            version: "1.0.0".to_string(),
            content_hash: None,
            capsule_type: CapsuleType::Fact,
            statement: "The sky is blue.".to_string(),
            epistemic_status: EpistemicStatus::Observed,
            confidence: Confidence {
                basis: ConfidenceBasis::DirectObservation,
                ceiling: 0.9,
                rationale: None,
            },
            lifecycle_state: LifecycleState::Draft,
            evidence: vec![],
            applicability: None,
            contradictions: vec![],
            dependencies: vec![],
            governance: Governance {
                owner: "alice".to_string(),
                domain: "domain".to_string(),
                permissions: vec![],
            },
            evaluation_contract: None,
        }
    }

    #[test]
    fn hash_is_stable_and_prefixed() {
        let capsule = minimal_capsule();
        let hash1 = content_hash(&capsule).unwrap();
        let hash2 = content_hash(&capsule).unwrap();
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("sha256:"));
        assert_eq!(hash1.len(), "sha256:".len() + 64);
    }

    #[test]
    fn hash_ignores_existing_content_hash_field() {
        let mut capsule = minimal_capsule();
        let hash_without = content_hash(&capsule).unwrap();
        capsule.content_hash = Some("sha256:deadbeef".to_string());
        let hash_with = content_hash(&capsule).unwrap();
        assert_eq!(hash_without, hash_with);
    }

    #[test]
    fn hash_changes_when_statement_changes() {
        let mut capsule = minimal_capsule();
        let original = content_hash(&capsule).unwrap();
        capsule.statement = "The sky is green.".to_string();
        let changed = content_hash(&capsule).unwrap();
        assert_ne!(original, changed);
    }

    #[test]
    fn identical_content_hashes_the_same_across_different_logical_ids() {
        let a = minimal_capsule();
        let mut b = minimal_capsule();
        b.logical_id = "kc://other-domain/other-topic/other-name".to_string();
        b.version = "3.4.5".to_string();
        b.lifecycle_state = LifecycleState::Published;
        b.governance = Governance {
            owner: "bob".to_string(),
            domain: "other-domain".to_string(),
            permissions: vec!["read".to_string()],
        };

        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn rehydrate_round_trips_the_original_capsule() {
        let original = minimal_capsule();
        let payload = content_payload(&original).unwrap();
        let rebuilt = rehydrate(
            &payload,
            &original.logical_id,
            &original.version,
            original.lifecycle_state,
            original.governance.clone(),
        )
        .unwrap();

        assert_eq!(rebuilt.statement, original.statement);
        assert_eq!(rebuilt.logical_id, original.logical_id);
        assert_eq!(content_hash(&rebuilt).unwrap(), content_hash(&original).unwrap());
    }
}
