//! `capsule-core`: Knowledge Capsule types, parsing, hashing, lifecycle
//! transitions, and epistemic validation for KCP 0.1.
//!
//! This crate targets **L0 (document conformance)** as defined in
//! `protocol/KCP-v0.1.md`: it can parse, validate, hash, and serialize a
//! capsule document. It does not implement storage, resolution, or
//! networked registry behaviour — those belong to later phases
//! (`capsule-registry`, `capsule-resolver`, `capsule-runtime`).

pub mod hashing;
pub mod lifecycle;
pub mod types;
pub mod validation;

pub use hashing::{canonicalize, content_hash, content_payload, rehydrate, source_fingerprint};
pub use lifecycle::{check_transition, InvalidTransition};
pub use types::*;
pub use validation::{is_valid, validate, Severity, ValidationIssue};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CapsuleError {
    #[error("failed to parse capsule JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Parses a capsule document from a JSON string. This only performs
/// structural (schema-shape) deserialization; call [`validate`] afterwards
/// to check semantic/epistemic conformance.
pub fn parse(json: &str) -> Result<Capsule, CapsuleError> {
    Ok(serde_json::from_str(json)?)
}

/// Serializes a capsule to pretty-printed JSON.
pub fn to_pretty_json(capsule: &Capsule) -> Result<String, CapsuleError> {
    Ok(serde_json::to_string_pretty(capsule)?)
}

/// Computes and stamps the `content_hash` field on a capsule, returning the
/// updated capsule. Idempotent: stamping twice yields the same hash because
/// hashing always excludes the existing `content_hash` value first.
pub fn stamp_content_hash(mut capsule: Capsule) -> Result<Capsule, CapsuleError> {
    let hash = content_hash(&capsule)?;
    capsule.content_hash = Some(hash);
    Ok(capsule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_bundled_example() {
        let json = include_str!("../../examples/capsules/data-retention-90-days.json");
        let capsule = parse(json).expect("bundled example must parse");
        assert!(is_valid(&capsule), "{:?}", validate(&capsule));
    }

    #[test]
    fn parses_and_validates_all_bundled_examples() {
        let fixtures = [
            include_str!("../../examples/capsules/data-retention-90-days.json"),
            include_str!("../../examples/capsules/data-retention-180-days.json"),
            include_str!("../../examples/capsules/marketing-incrementality-estimate.json"),
            include_str!("../../examples/capsules/marketing-incrementality-counterexample.json"),
            include_str!("../../examples/capsules/legal-hold-procedure.json"),
            include_str!("../../examples/capsules/legal-hold-interpretation.json"),
            include_str!("../../examples/capsules/research-dataset-boundary.json"),
            include_str!("../../examples/capsules/retention-answer-rubric.json"),
        ];
        for json in fixtures {
            let capsule = parse(json).expect("bundled example must parse");
            assert!(is_valid(&capsule), "{:?}", validate(&capsule));
        }
    }

    #[test]
    fn stamping_then_validating_round_trips() {
        let json = include_str!("../../examples/capsules/data-retention-90-days.json");
        let capsule = parse(json).unwrap();
        let stamped = stamp_content_hash(capsule).unwrap();
        assert!(is_valid(&stamped), "{:?}", validate(&stamped));
        assert!(stamped.content_hash.unwrap().starts_with("sha256:"));
    }
}
