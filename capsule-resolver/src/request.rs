//! The resolution request contract (KCP §5), and a parser for the
//! Capsule Query Language `RESOLVE` form.

use capsule_core::{CapsuleType, EvidenceAuthority, LifecycleState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Include {
    /// Boundary and counterexample capsules that limit the resolved ones.
    Exceptions,
    /// Unresolved contradictions, bundled with what they contradict.
    Contradictions,
    /// `requires` / `refines` dependencies, transitively.
    Dependencies,
}

/// Who is asking. Capsules whose `governance.permissions` do not include
/// one of `permissions` (or whose domain is outside `domains`, when set)
/// are filtered out before anything else is considered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requester {
    pub id: String,
    #[serde(default = "default_permissions")]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
}

fn default_permissions() -> Vec<String> {
    vec!["read".into()]
}

/// Minimum evidence for a capsule to be resolved as authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceGate {
    /// At least one evidence item of any authority.
    Any,
    /// At least one item at `secondary` or better.
    Secondary,
    /// At least one `primary` item.
    Primary,
}

impl EvidenceGate {
    pub fn admits(&self, a: EvidenceAuthority) -> bool {
        match self {
            EvidenceGate::Any => true,
            EvidenceGate::Secondary => matches!(a, EvidenceAuthority::Primary | EvidenceAuthority::Secondary),
            EvidenceGate::Primary => a == EvidenceAuthority::Primary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionRequest {
    pub purpose: String,
    pub about: String,
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// `YYYY-MM-DD`; defaults to the resolver's clock date.
    #[serde(default)]
    pub as_of: Option<String>,
    #[serde(default = "default_status")]
    pub require_status: Vec<LifecycleState>,
    #[serde(default)]
    pub include: Vec<Include>,
    /// Restrict to these capsule types (otherwise planned from purpose).
    #[serde(default)]
    pub capsule_types: Vec<CapsuleType>,
    #[serde(default)]
    pub requester: Option<Requester>,
    #[serde(default = "default_gate")]
    pub evidence_gate: EvidenceGate,
    /// Facts about the situation, matched against capsule conditions
    /// (e.g. "account has been formally closed").
    #[serde(default)]
    pub context: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_status() -> Vec<LifecycleState> {
    vec![LifecycleState::Published]
}
fn default_gate() -> EvidenceGate {
    EvidenceGate::Any
}
fn default_limit() -> usize {
    10
}

impl ResolutionRequest {
    pub fn new(purpose: &str, about: &str) -> Self {
        Self {
            purpose: purpose.into(),
            about: about.into(),
            jurisdiction: None,
            as_of: None,
            require_status: default_status(),
            include: vec![],
            capsule_types: vec![],
            requester: None,
            evidence_gate: default_gate(),
            context: vec![],
            limit: default_limit(),
        }
    }

    pub fn includes(&self, i: Include) -> bool {
        self.include.contains(&i)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("CQL line {line}: {message}")]
pub struct CqlError {
    pub line: usize,
    pub message: String,
}

fn quoted(s: &str) -> Option<String> {
    let s = s.trim();
    s.strip_prefix('"').and_then(|r| r.strip_suffix('"')).map(str::to_string)
}

fn capsule_type(word: &str) -> Option<CapsuleType> {
    let w = word.to_lowercase();
    let w = w.trim_end_matches('s');
    let w = match w {
        "policy_rule" | "policy" | "rule" => "rule",
        "exception" => "boundary",
        other => other,
    };
    serde_json::from_value(serde_json::Value::String(w.to_string())).ok()
}

/// Parses the CQL `RESOLVE` statement from the top-level README:
///
/// ```text
/// RESOLVE policy_rule
/// ABOUT "customer data retention"
/// FOR purpose "privacy review"
/// IN jurisdiction "India"
/// AS OF "2026-08-01"
/// REQUIRE status PUBLISHED
/// INCLUDE exceptions
/// INCLUDE contradictions
/// WITH dependencies
/// ```
pub fn parse_cql(src: &str) -> Result<ResolutionRequest, CqlError> {
    let mut req = ResolutionRequest::new("", "");
    req.require_status.clear();
    let mut saw_resolve = false;
    for (n, raw) in src.lines().enumerate() {
        let line = raw.trim();
        let err = |m: &str| CqlError { line: n + 1, message: m.to_string() };
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let upper = line.to_uppercase();
        if let Some(rest) = upper.strip_prefix("RESOLVE") {
            saw_resolve = true;
            for w in line[line.len() - rest.len()..].split([',', ' ']).filter(|w| !w.is_empty()) {
                if w.eq_ignore_ascii_case("any") {
                    continue;
                }
                req.capsule_types.push(capsule_type(w).ok_or_else(|| err(&format!("unknown capsule type '{w}'")))?);
            }
        } else if upper.starts_with("ABOUT ") {
            req.about = quoted(&line[6..]).ok_or_else(|| err("ABOUT needs a quoted topic"))?;
        } else if upper.starts_with("FOR PURPOSE ") {
            req.purpose = quoted(&line[12..]).ok_or_else(|| err("FOR purpose needs a quoted value"))?;
        } else if upper.starts_with("IN JURISDICTION ") {
            req.jurisdiction = Some(quoted(&line[16..]).ok_or_else(|| err("IN jurisdiction needs a quoted value"))?);
        } else if upper.starts_with("AS OF ") {
            let d = quoted(&line[6..]).ok_or_else(|| err("AS OF needs a quoted date"))?;
            if d.len() != 10 || d.as_bytes()[4] != b'-' || d.as_bytes()[7] != b'-' {
                return Err(err("AS OF must be YYYY-MM-DD"));
            }
            req.as_of = Some(d);
        } else if upper.starts_with("REQUIRE STATUS ") {
            for w in line[15..].split([',', ' ']).filter(|w| !w.is_empty()) {
                let s: LifecycleState = serde_json::from_value(serde_json::Value::String(w.to_lowercase()))
                    .map_err(|_| err(&format!("unknown status '{w}'")))?;
                req.require_status.push(s);
            }
        } else if upper.starts_with("REQUIRE EVIDENCE ") {
            req.evidence_gate = match line[17..].trim().to_lowercase().as_str() {
                "any" => EvidenceGate::Any,
                "secondary" => EvidenceGate::Secondary,
                "primary" => EvidenceGate::Primary,
                other => return Err(err(&format!("unknown evidence gate '{other}'"))),
            };
        } else if upper.starts_with("INCLUDE ") || upper.starts_with("WITH ") {
            let what = line.split_once(' ').map(|x| x.1).unwrap_or("").trim().to_lowercase();
            req.include.push(match what.as_str() {
                "exceptions" => Include::Exceptions,
                "contradictions" => Include::Contradictions,
                "dependencies" => Include::Dependencies,
                other => return Err(err(&format!("cannot include '{other}'"))),
            });
        } else if upper.starts_with("GIVEN ") {
            req.context.push(quoted(&line[6..]).ok_or_else(|| err("GIVEN needs a quoted fact"))?);
        } else if upper.starts_with("LIMIT ") {
            req.limit = line[6..].trim().parse().map_err(|_| err("LIMIT needs a number"))?;
        } else {
            return Err(err(&format!("unrecognized clause '{line}'")));
        }
    }
    if !saw_resolve {
        return Err(CqlError { line: 1, message: "a query starts with RESOLVE".into() });
    }
    if req.about.is_empty() {
        return Err(CqlError { line: 1, message: "ABOUT is required".into() });
    }
    if req.require_status.is_empty() {
        req.require_status = default_status();
    }
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_readme_query() {
        let q = parse_cql(
            r#"RESOLVE policy_rule
ABOUT "customer data retention"
FOR purpose "privacy review"
IN jurisdiction "India"
AS OF "2026-08-01"
REQUIRE status PUBLISHED
INCLUDE exceptions
INCLUDE contradictions
WITH dependencies"#,
        )
        .unwrap();
        assert_eq!(q.capsule_types, vec![CapsuleType::Rule]);
        assert_eq!(q.about, "customer data retention");
        assert_eq!(q.purpose, "privacy review");
        assert_eq!(q.jurisdiction.as_deref(), Some("India"));
        assert_eq!(q.as_of.as_deref(), Some("2026-08-01"));
        assert_eq!(q.require_status, vec![LifecycleState::Published]);
        assert_eq!(q.include, vec![Include::Exceptions, Include::Contradictions, Include::Dependencies]);
    }

    #[test]
    fn rejects_malformed_queries() {
        assert!(parse_cql("ABOUT \"x\"").is_err());
        assert_eq!(parse_cql("RESOLVE rule\nABOUT x").unwrap_err().line, 2);
        assert!(parse_cql("RESOLVE wizard\nABOUT \"x\"").is_err());
        assert!(parse_cql("RESOLVE rule\nABOUT \"x\"\nAS OF \"Aug 1\"").is_err());
    }
}
