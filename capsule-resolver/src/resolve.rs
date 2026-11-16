//! Resolution: from a request and a candidate source to a resolution
//! bundle with an inspectable plan.
//!
//! ```text
//! candidates
//!   → identity / permission filter
//!   → required status
//!   → temporal version resolution (as_of window, newest eligible version)
//!   → discovery (lexical, optional semantic) for the `about` topic
//!   → capsule-type planning from purpose
//!   → applicability (jurisdiction, exclusions vs context; conditions reported)
//!   → evidence-sufficiency gate
//!   → authority ordering, limit
//!   → dependency completion (with ceiling propagation)
//!   → exceptions and contradiction bundles
//!   → known gaps
//! ```
//!
//! Every candidate gets a plan entry saying where it was kept or dropped
//! and why. The bundle carries digests of the request, the candidate set,
//! and the result, so it can be replayed later and any drift detected.

use crate::index::*;
use crate::request::{Include, ResolutionRequest};
use capsule_core::{CapsuleType, DependencyRelation, EpistemicStatus, EvidenceAuthority};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Directly answers the request.
    Primary,
    /// Required (transitively) by a primary capsule.
    Dependency,
    /// A boundary or counterexample limiting a resolved capsule.
    Exception,
    /// In unresolved contradiction with a resolved capsule.
    Contradiction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub key: String,
    pub role: Role,
    pub rank: usize,
    pub relevance: f64,
    pub content_hash: String,
    /// Ceiling after `requires` propagation (spec §6.3).
    pub effective_ceiling: f64,
    /// Capsule conditions the request context did not establish.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions_unverified: Vec<String>,
    pub capsule: capsule_core::Capsule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContradictionBundle {
    pub capsules: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub step: String,
    pub capsule: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionBundle {
    pub request: ResolutionRequest,
    pub as_of: String,
    pub planned_types: Vec<CapsuleType>,
    pub resolved: Vec<Resolved>,
    pub contradictions: Vec<ContradictionBundle>,
    pub known_gaps: Vec<Gap>,
    pub plan: Vec<PlanEntry>,
    pub request_digest: String,
    pub source_digest: String,
    pub result_digest: String,
}

fn sha(s: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(s.as_bytes()))
}

/// Capsule types worth resolving for a purpose, most relevant first.
pub fn plan_types(purpose: &str) -> Vec<CapsuleType> {
    use CapsuleType::*;
    let p = purpose.to_lowercase();
    let any = |ws: &[&str]| ws.iter().any(|w| p.contains(w));
    if any(&["review", "compliance", "policy", "legal", "privacy", "audit", "regulat"]) {
        vec![Rule, Boundary, Procedure, Fact, Interpretation]
    } else if any(&["evaluat", "grading", "assess", "scor"]) {
        vec![Rubric, Rule, Fact, Metric]
    } else if any(&["how to", "procedure", "operat", "runbook", "process"]) {
        vec![Procedure, Rule, Boundary]
    } else if any(&["forecast", "plan", "budget", "analys", "decision", "simulat", "estimate"]) {
        vec![Metric, Interpretation, Fact, Counterexample]
    } else {
        vec![Rule, Fact, Metric, Procedure, Interpretation, Boundary, Counterexample, Rubric]
    }
}

const JURISDICTION_NAMES: &[(&str, &str)] = &[
    ("india", "IN"),
    ("european union", "EU"),
    ("europe", "EU"),
    ("united states", "US"),
    ("usa", "US"),
    ("united kingdom", "GB"),
    ("uk", "GB"),
    ("germany", "DE"),
    ("france", "FR"),
    ("singapore", "SG"),
    ("japan", "JP"),
    ("canada", "CA"),
    ("australia", "AU"),
    ("brazil", "BR"),
];

fn jurisdiction_code(j: &str) -> String {
    let l = j.trim().to_lowercase();
    JURISDICTION_NAMES.iter().find(|(n, _)| *n == l).map(|(_, c)| c.to_string()).unwrap_or_else(|| j.trim().to_uppercase())
}

fn authority_rank(c: &capsule_core::Capsule) -> u8 {
    c.evidence
        .iter()
        .map(|e| match e.authority {
            EvidenceAuthority::Primary => 3,
            EvidenceAuthority::Secondary => 2,
            EvidenceAuthority::Derived => 1,
            EvidenceAuthority::Unverified => 0,
        })
        .max()
        .unwrap_or(0)
}

fn status_rank(s: EpistemicStatus) -> u8 {
    match s {
        EpistemicStatus::PolicyDefined | EpistemicStatus::Observed => 3,
        EpistemicStatus::Inferred | EpistemicStatus::Estimated | EpistemicStatus::Modelled => 2,
        EpistemicStatus::ExpertJudgement => 1,
        EpistemicStatus::Disputed | EpistemicStatus::Unknown => 0,
    }
}

fn in_window(c: &capsule_core::Capsule, as_of: &str) -> bool {
    let Some(a) = &c.applicability else { return true };
    a.valid_from.as_deref().is_none_or(|f| f <= as_of) && a.valid_until.as_deref().is_none_or(|u| u >= as_of)
}

/// Relevance below this is not a match for the `about` topic.
pub const MIN_RELEVANCE: f64 = 0.34;

pub struct Resolver<'a> {
    pub semantic: Option<&'a dyn SemanticIndex>,
    /// Today's date (`YYYY-MM-DD`) when the request has no `as_of`.
    pub today: String,
}

impl<'a> Resolver<'a> {
    pub fn new(today: &str) -> Self {
        Self { semantic: None, today: today.into() }
    }

    pub fn resolve(&self, req: &ResolutionRequest, source: &dyn CapsuleSource) -> Result<ResolutionBundle, SourceError> {
        let mut all = source.candidates()?;
        all.sort_by_key(|c| c.key());
        let as_of = req.as_of.clone().unwrap_or_else(|| self.today.clone());
        let mut plan = Vec::new();
        let mut note = |step: &str, key: &str, decision: &str, reason: String| {
            plan.push(PlanEntry { step: step.into(), capsule: key.into(), decision: decision.into(), reason });
        };
        let mut gaps = Vec::new();

        // 1. identity and permissions
        let mut pool: Vec<Candidate> = Vec::new();
        for c in &all {
            let g = &c.capsule.governance;
            let ok = match &req.requester {
                None => true,
                Some(r) => {
                    (g.permissions.is_empty() || g.permissions.iter().any(|p| r.permissions.contains(p)))
                        && (r.domains.is_empty() || r.domains.contains(&g.domain))
                }
            };
            if ok {
                pool.push(c.clone());
            } else {
                note("permission", &c.key(), "excluded", "requester lacks permission or domain access".into());
            }
        }
        let visible = pool.clone();

        // 2. status, 3. temporal version resolution
        let mut eligible: BTreeMap<String, Candidate> = BTreeMap::new();
        for c in pool {
            if !req.require_status.contains(&c.lifecycle_state) {
                note("status", &c.key(), "excluded", format!("state {:?} is not required", c.lifecycle_state).to_lowercase());
                continue;
            }
            if !in_window(&c.capsule, &as_of) {
                note("temporal", &c.key(), "excluded", format!("not valid on {as_of}"));
                continue;
            }
            let id = c.capsule.logical_id.clone();
            let newer = match eligible.get(&id) {
                None => true,
                Some(prev) => match (semver::Version::parse(&c.capsule.version), semver::Version::parse(&prev.capsule.version)) {
                    (Ok(a), Ok(b)) => a > b,
                    _ => false,
                },
            };
            if newer {
                if let Some(prev) = eligible.insert(id, c) {
                    note("temporal", &prev.key(), "excluded", "a newer eligible version exists".into());
                }
            } else {
                note("temporal", &c.key(), "excluded", "a newer eligible version exists".into());
            }
        }
        // Explicit supersession between different logical ids.
        let superseded: BTreeSet<String> = eligible
            .values()
            .flat_map(|c| c.capsule.dependencies.iter().filter(|d| d.relation == DependencyRelation::Supersedes).map(|d| d.capsule_id.clone()))
            .collect();
        let eligible: Vec<Candidate> = eligible
            .into_values()
            .filter(|c| {
                let gone = superseded.iter().any(|s| refers_to(s, c));
                if gone {
                    note("temporal", &c.key(), "excluded", "superseded by an eligible capsule".into());
                }
                !gone
            })
            .collect();

        // 4. discovery
        let lexical = LexicalIndex::build(&eligible);
        let lexical_hits = lexical.search(&req.about);
        let mut relevance: BTreeMap<String, f64> = BTreeMap::new();
        for c in &eligible {
            let mut s = lexical_hits.get(&c.key()).copied().unwrap_or(0.0);
            if let Some(sem) = self.semantic {
                s = s.max(sem.score(&req.about, &c.capsule));
            }
            relevance.insert(c.key(), s);
        }

        // 5. type planning
        let planned = if req.capsule_types.is_empty() { plan_types(&req.purpose) } else { req.capsule_types.clone() };
        let exception_types = [CapsuleType::Boundary, CapsuleType::Counterexample];

        // 6. applicability, 7. evidence gate
        let context_terms: Vec<BTreeSet<String>> = req.context.iter().map(|c| terms(c)).collect();
        let fact_holds = |text: &str| {
            let t = terms(text);
            context_terms.iter().any(|ct| coverage(&t, ct) >= 0.6)
        };
        let jurisdiction = req.jurisdiction.as_deref().map(jurisdiction_code);
        let mut primaries: Vec<(Candidate, f64, Vec<String>)> = Vec::new();
        let mut exceptions_pool: Vec<Candidate> = Vec::new();
        for c in &eligible {
            let key = c.key();
            let rel = relevance[&key];
            if rel < MIN_RELEVANCE {
                note("discovery", &key, "excluded", format!("relevance {rel:.2} to \"{}\" is below {MIN_RELEVANCE}", req.about));
                continue;
            }
            let ty = c.capsule.capsule_type;
            let is_exception_type = exception_types.contains(&ty);
            if !(planned.contains(&ty) || req.includes(Include::Exceptions) && is_exception_type) {
                note("type_planning", &key, "excluded", format!("{ty:?} is not planned for purpose \"{}\"", req.purpose).to_lowercase());
                continue;
            }
            if let (Some(j), Some(app)) = (&jurisdiction, &c.capsule.applicability) {
                if !app.jurisdictions.is_empty() && !app.jurisdictions.iter().any(|x| x.eq_ignore_ascii_case(j)) {
                    note("applicability", &key, "excluded", format!("applies to {}, not {j}", app.jurisdictions.join(", ")));
                    continue;
                }
            }
            if let Some(ex) = c.capsule.applicability.as_ref().and_then(|a| a.exclusions.iter().find(|e| fact_holds(e))) {
                note("applicability", &key, "excluded", format!("the context falls under its exclusion \"{ex}\""));
                continue;
            }
            let unverified: Vec<String> = c
                .capsule
                .applicability
                .as_ref()
                .map(|a| a.conditions.iter().filter(|cond| !fact_holds(cond)).cloned().collect())
                .unwrap_or_default();
            if !c.capsule.evidence.iter().any(|e| req.evidence_gate.admits(e.authority)) {
                note("evidence_gate", &key, "excluded", format!("no evidence meets the {:?} gate", req.evidence_gate).to_lowercase());
                gaps.push(Gap { kind: "evidence_insufficient".into(), detail: format!("{key} matched but lacks sufficient evidence") });
                continue;
            }
            if planned.contains(&ty) {
                primaries.push((c.clone(), rel, unverified));
            } else {
                exceptions_pool.push(c.clone());
            }
        }

        // 8. authority ordering
        let type_rank = |t: CapsuleType| planned.iter().position(|p| *p == t).unwrap_or(planned.len());
        primaries.sort_by(|(a, ra, _), (b, rb, _)| {
            type_rank(a.capsule.capsule_type)
                .cmp(&type_rank(b.capsule.capsule_type))
                .then(authority_rank(&b.capsule).cmp(&authority_rank(&a.capsule)))
                .then(status_rank(b.capsule.epistemic_status).cmp(&status_rank(a.capsule.epistemic_status)))
                .then(rb.total_cmp(ra))
                .then(b.capsule.confidence.ceiling.total_cmp(&a.capsule.confidence.ceiling))
                .then(a.key().cmp(&b.key()))
        });
        for (c, _, _) in primaries.iter().skip(req.limit) {
            note("ordering", &c.key(), "excluded", format!("beyond the limit of {}", req.limit));
        }
        primaries.truncate(req.limit);

        let graph = GraphIndex::build(&visible);
        let find = |reference: &str| -> Option<&Candidate> {
            // Prefer an eligible version; otherwise any visible one.
            eligible.iter().find(|c| refers_to(reference, c)).or_else(|| visible.iter().find(|c| refers_to(reference, c)))
        };

        let mut resolved: Vec<Resolved> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut push = |c: &Candidate, role: Role, relevance: f64, unverified: Vec<String>, resolved: &mut Vec<Resolved>| {
            if seen.insert(c.key()) {
                resolved.push(Resolved {
                    key: c.key(),
                    role,
                    rank: resolved.len() + 1,
                    relevance,
                    content_hash: c.content_hash.clone(),
                    effective_ceiling: c.capsule.confidence.ceiling,
                    conditions_unverified: unverified,
                    capsule: c.capsule.clone(),
                });
                true
            } else {
                false
            }
        };
        for (c, rel, unverified) in &primaries {
            push(c, Role::Primary, *rel, unverified.clone(), &mut resolved);
            let mut reason = format!("relevance {rel:.2}, {:?}, evidence authority {}", c.capsule.capsule_type, authority_rank(&c.capsule));
            if !unverified.is_empty() {
                reason.push_str(&format!("; conditions not established: {}", unverified.join("; ")));
            }
            note("ordering", &c.key(), "included", reason.to_lowercase());
        }

        // 10. dependency completion
        if req.includes(Include::Dependencies) {
            let mut queue: Vec<String> = resolved.iter().map(|r| r.key.clone()).collect();
            while let Some(k) = queue.pop() {
                for e in graph.dependencies_of(&k).filter(|e| e.relation != DependencyRelation::Supersedes).cloned().collect::<Vec<_>>() {
                    match find(&e.to) {
                        Some(dep) if req.require_status.contains(&dep.lifecycle_state) && in_window(&dep.capsule, &as_of) => {
                            if push(dep, Role::Dependency, 0.0, vec![], &mut resolved) {
                                note("dependencies", &dep.key(), "included", format!("{:?} by {k}", e.relation).to_lowercase());
                                queue.push(dep.key());
                            }
                        }
                        Some(dep) => {
                            note("dependencies", &dep.key(), "excluded", format!("required by {k} but not in force on {as_of}"));
                            gaps.push(Gap { kind: "unmet_dependency".into(), detail: format!("{k} depends on {} which is not in force", dep.key()) });
                        }
                        None => gaps.push(Gap { kind: "unmet_dependency".into(), detail: format!("{k} depends on {} which is not available", e.to) }),
                    }
                }
            }
            // Ceiling propagation over `requires`.
            let ceilings: BTreeMap<String, f64> = resolved.iter().map(|r| (r.key.clone(), r.capsule.confidence.ceiling)).collect();
            for r in resolved.iter_mut() {
                let mut stack = vec![r.key.clone()];
                let mut visited = BTreeSet::new();
                let mut min = r.effective_ceiling;
                while let Some(k) = stack.pop() {
                    if !visited.insert(k.clone()) {
                        continue;
                    }
                    for e in graph.dependencies_of(&k).filter(|e| e.relation == DependencyRelation::Requires) {
                        if let Some(dep) = find(&e.to) {
                            if let Some(c) = ceilings.get(&dep.key()) {
                                min = min.min(*c);
                            }
                            stack.push(dep.key());
                        }
                    }
                }
                r.effective_ceiling = min;
            }
        }

        // 11. exceptions and contradictions
        let mut bundles = Vec::new();
        let anchors: Vec<String> = resolved.iter().map(|r| r.key.clone()).collect();
        if req.includes(Include::Contradictions) || req.includes(Include::Exceptions) {
            for (from, to, description) in &graph.contradictions {
                // Only capsules in force on `as_of` can contradict.
                let from_c = eligible.iter().find(|c| &c.key() == from);
                let to_c = eligible.iter().find(|c| refers_to(to, c));
                let (Some(a), Some(b)) = (from_c, to_c) else { continue };
                let (anchor, other) = if anchors.contains(&a.key()) {
                    (a, b)
                } else if anchors.contains(&b.key()) {
                    (b, a)
                } else {
                    continue;
                };
                let is_exception = exception_types.contains(&other.capsule.capsule_type);
                if req.includes(Include::Contradictions) {
                    bundles.push(ContradictionBundle { capsules: vec![anchor.key(), other.key()], description: description.clone() });
                }
                let role = if is_exception && req.includes(Include::Exceptions) { Role::Exception } else { Role::Contradiction };
                if (req.includes(Include::Contradictions) || role == Role::Exception) && push(other, role, 0.0, vec![], &mut resolved) {
                    note("contradictions", &other.key(), "included", format!("unresolved contradiction with {}: {description}", anchor.key()));
                }
            }
        }
        if req.includes(Include::Exceptions) {
            for c in &exceptions_pool {
                if push(c, Role::Exception, relevance[&c.key()], vec![], &mut resolved) {
                    note("exceptions", &c.key(), "included", "boundary or counterexample on the same topic".into());
                }
            }
        }

        // 12. known gaps
        let about = terms(&req.about);
        let covered: BTreeSet<String> = resolved.iter().filter_map(|r| lexical.terms_of(&r.key)).flatten().cloned().collect();
        let uncovered: Vec<&String> = about.iter().filter(|t| !covered.contains(*t)).collect();
        if resolved.is_empty() {
            gaps.push(Gap { kind: "no_match".into(), detail: format!("no eligible capsule addresses \"{}\"", req.about) });
        } else if !uncovered.is_empty() {
            gaps.push(Gap {
                kind: "uncovered_terms".into(),
                detail: format!("no resolved capsule mentions: {}", uncovered.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
            });
        }
        if !req.capsule_types.is_empty() {
            for t in &req.capsule_types {
                if !resolved.iter().any(|r| r.capsule.capsule_type == *t) {
                    gaps.push(Gap { kind: "missing_type".into(), detail: format!("no {t:?} capsule resolved").to_lowercase() });
                }
            }
        }

        let request_digest = sha(&serde_json::to_string(req).unwrap_or_default());
        let source_digest = sha(&all.iter().map(|c| format!("{}={}:{:?}", c.key(), c.content_hash, c.lifecycle_state)).collect::<Vec<_>>().join("\n"));
        let result = serde_json::json!({
            "as_of": as_of,
            "resolved": resolved.iter().map(|r| (r.key.clone(), r.role, r.rank, r.content_hash.clone())).collect::<Vec<_>>(),
            "contradictions": bundles,
            "gaps": gaps,
        });
        Ok(ResolutionBundle {
            request: req.clone(),
            as_of,
            planned_types: planned,
            resolved,
            contradictions: bundles,
            known_gaps: gaps,
            plan,
            request_digest,
            source_digest,
            result_digest: sha(&result.to_string()),
        })
    }

    /// Re-runs a recorded resolution against a source and reports whether
    /// it still produces the same result.
    pub fn replay(&self, recorded: &ResolutionBundle, source: &dyn CapsuleSource) -> Result<Replay, SourceError> {
        let mut req = recorded.request.clone();
        req.as_of = Some(recorded.as_of.clone());
        let now = self.resolve(&req, source)?;
        let before: Vec<(String, Role)> = recorded.resolved.iter().map(|r| (r.key.clone(), r.role)).collect();
        let after: Vec<(String, Role)> = now.resolved.iter().map(|r| (r.key.clone(), r.role)).collect();
        let mut differences = Vec::new();
        for (k, role) in &before {
            if !after.iter().any(|(a, _)| a == k) {
                differences.push(format!("no longer resolved: {k} ({role:?})").to_lowercase());
            }
        }
        for (k, role) in &after {
            if !before.iter().any(|(b, _)| b == k) {
                differences.push(format!("newly resolved: {k} ({role:?})").to_lowercase());
            }
        }
        if differences.is_empty() && now.result_digest != recorded.result_digest {
            differences.push("same capsules, but order, content, contradictions, or gaps changed".into());
        }
        Ok(Replay {
            identical: now.result_digest == recorded.result_digest,
            source_changed: now.source_digest != recorded.source_digest,
            differences,
            current: now,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub identical: bool,
    pub source_changed: bool,
    pub differences: Vec<String>,
    pub current: ResolutionBundle,
}
