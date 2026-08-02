//! Canonical serialization and content-addressed hashing (spec §2.3).

use crate::types::Capsule;
use sha2::{Digest, Sha256};

/// Serializes a capsule to its canonical JSON form: UTF-8, object keys
/// sorted lexicographically, no insignificant whitespace, and the
/// `content_hash` field itself excluded.
///
/// Relies on `serde_json::Map` being backed by a `BTreeMap` (the default,
/// i.e. the `preserve_order` feature is *not* enabled anywhere in the
/// dependency graph), which sorts keys by byte order during serialization.
pub fn canonicalize(capsule: &Capsule) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(capsule)?;
    if let serde_json::Value::Object(map) = &mut value {
        map.remove("content_hash");
    }
    serde_json::to_string(&value)
}

/// Computes the `sha256:<hex>` content hash of a capsule's canonical form.
pub fn content_hash(capsule: &Capsule) -> serde_json::Result<String> {
    let canonical = canonicalize(capsule)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

/// Fingerprints arbitrary source material (e.g. an evidence excerpt or a
/// whole source document) so provenance can be checked without re-storing
/// the source itself.
pub fn source_fingerprint(source_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_bytes);
    format!("sha256:{:x}", hasher.finalize())
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
}
