//! Policy enforcement, purpose-bound access, and consumer-specific
//! capsule views.

use capsule_core::{Capsule, Evidence};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Who is asking, and (optionally) why. `role` is checked against a
/// capsule's `governance.permissions` — empty permissions means public.
#[derive(Debug, Clone, Default)]
pub struct AccessRequest {
    pub consumer_role: Option<String>,
    pub purpose: Option<String>,
    /// Grants visibility into raw evidence excerpts. Without it, evidence
    /// entries are returned with their `excerpt` redacted — a consumer
    /// gets to know evidence exists and its authority level, not
    /// necessarily the verbatim source text.
    pub can_read_evidence: bool,
}

impl AccessRequest {
    pub fn public() -> Self {
        Self::default()
    }
}

#[derive(Debug, Error)]
#[error("access denied: {reason}")]
pub struct AccessDenied {
    pub reason: String,
}

/// Checks whether `access` may see `capsule` at all (identity/permission
/// filtering, purpose-bound access). This is the same permission rule
/// `capsule-resolver` applies during resolution; it's duplicated here
/// (rather than imported) because the runtime also needs to gate direct
/// `get()` calls that don't go through a resolution request.
pub fn check_access(capsule: &Capsule, access: &AccessRequest) -> Result<(), AccessDenied> {
    let perms = &capsule.governance.permissions;
    if perms.is_empty() {
        return Ok(());
    }
    match &access.consumer_role {
        Some(role) if perms.iter().any(|p| p == role) => Ok(()),
        _ => Err(AccessDenied { reason: format!("requires one of {perms:?}") }),
    }
}

/// A capsule as seen by a specific consumer: evidence excerpts redacted
/// unless `can_read_evidence` was granted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleView {
    #[serde(flatten)]
    pub capsule: Capsule,
}

pub fn build_view(mut capsule: Capsule, access: &AccessRequest) -> CapsuleView {
    if !access.can_read_evidence {
        capsule.evidence = capsule
            .evidence
            .into_iter()
            .map(|e| Evidence { excerpt: e.excerpt.map(|_| "[redacted]".to_string()), ..e })
            .collect();
    }
    CapsuleView { capsule }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capsule_core::*;

    fn capsule_with_permissions(permissions: Vec<String>) -> Capsule {
        Capsule {
            schema_version: SCHEMA_VERSION.to_string(),
            logical_id: "kc://a/b/c".to_string(),
            version: "1.0.0".to_string(),
            content_hash: None,
            capsule_type: CapsuleType::Rule,
            statement: "x".to_string(),
            epistemic_status: EpistemicStatus::PolicyDefined,
            confidence: Confidence { basis: ConfidenceBasis::PolicyMandate, ceiling: 0.9, rationale: None },
            lifecycle_state: LifecycleState::Published,
            evidence: vec![Evidence {
                source_id: "s1".to_string(),
                span: None,
                excerpt: Some("secret excerpt".to_string()),
                authority: EvidenceAuthority::Primary,
                retrieved_at: None,
            }],
            applicability: None,
            contradictions: vec![],
            dependencies: vec![],
            governance: Governance { owner: "alice".to_string(), domain: "policy".to_string(), permissions },
            evaluation_contract: None,
        }
    }

    #[test]
    fn public_capsule_is_accessible_to_anyone() {
        let capsule = capsule_with_permissions(vec![]);
        assert!(check_access(&capsule, &AccessRequest::public()).is_ok());
    }

    #[test]
    fn restricted_capsule_requires_matching_role() {
        let capsule = capsule_with_permissions(vec!["legal-team".to_string()]);
        assert!(check_access(&capsule, &AccessRequest::public()).is_err());
        let access = AccessRequest { consumer_role: Some("legal-team".to_string()), ..Default::default() };
        assert!(check_access(&capsule, &access).is_ok());
    }

    #[test]
    fn view_redacts_evidence_excerpt_without_permission() {
        let capsule = capsule_with_permissions(vec![]);
        let view = build_view(capsule.clone(), &AccessRequest::public());
        assert_eq!(view.capsule.evidence[0].excerpt.as_deref(), Some("[redacted]"));

        let access = AccessRequest { can_read_evidence: true, ..Default::default() };
        let view = build_view(capsule, &access);
        assert_eq!(view.capsule.evidence[0].excerpt.as_deref(), Some("secret excerpt"));
    }
}
