use crate::error::ResolverError;
use crate::types::*;
use capsule_core::{Capsule, DependencyRelation, EvidenceAuthority};
use capsule_registry::{CapsuleRecord, DependencyRecord, Registry};
use std::collections::{HashMap, HashSet};

pub struct Resolver<'a> {
    registry: &'a Registry,
}

impl<'a> Resolver<'a> {
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Resolves the applicable capsule set for `request`: filters every
    /// registered logical ID by domain/type/keyword/jurisdiction/
    /// permission/evidence-sufficiency, orders survivors by evidence
    /// authority, completes `requires` dependencies (propagating the
    /// confidence-ceiling rule from spec §6.3), bundles contradictions,
    /// and records every inclusion/exclusion decision in an inspectable,
    /// serializable plan.
    pub fn resolve(&self, request: &ResolutionRequest) -> Result<ResolutionBundle, ResolverError> {
        let mut plan = Vec::new();
        let mut primary_records: Vec<CapsuleRecord> = Vec::new();

        for logical_id in self.registry.list_logical_ids()? {
            if let Some(domain) = &request.domain {
                if !logical_id.starts_with(&format!("kc://{domain}/")) {
                    continue;
                }
            }

            let Some(record) = self.registry.latest_published(&logical_id, request.as_of)? else {
                plan.push(PlanStep {
                    logical_id: logical_id.clone(),
                    version: None,
                    decision: Decision::Excluded,
                    reason: "no published version applicable as of the requested date".to_string(),
                });
                continue;
            };

            if let Some(types) = &request.capsule_types {
                if !types.contains(&record.capsule.capsule_type) {
                    plan.push(exclude(&record, "capsule_type not in requested set"));
                    continue;
                }
            }

            if let Some(about) = &request.about {
                if !record.capsule.statement.to_lowercase().contains(&about.to_lowercase()) {
                    plan.push(exclude(&record, "statement does not match 'about' keyword"));
                    continue;
                }
            }

            if let Some(jurisdiction) = &request.jurisdiction {
                let applies = record
                    .capsule
                    .applicability
                    .as_ref()
                    .map(|a| a.jurisdictions.is_empty() || a.jurisdictions.iter().any(|j| j.eq_ignore_ascii_case(jurisdiction)))
                    .unwrap_or(true);
                if !applies {
                    plan.push(exclude(&record, "jurisdiction not applicable"));
                    continue;
                }
            }

            if let Some(role) = &request.consumer_role {
                let perms = &record.capsule.governance.permissions;
                if !perms.is_empty() && !perms.iter().any(|p| p == role) {
                    plan.push(exclude(&record, "consumer role not permitted"));
                    continue;
                }
            }

            let sufficient = record.capsule.evidence.iter().any(|e| e.authority != EvidenceAuthority::Unverified);
            if request.require_sufficient_evidence && !sufficient {
                plan.push(exclude(&record, "evidence-sufficiency gate: all evidence is unverified"));
                continue;
            }

            plan.push(PlanStep {
                logical_id: record.logical_id.clone(),
                version: Some(record.version.clone()),
                decision: Decision::Included,
                reason: "matched request criteria".to_string(),
            });
            primary_records.push(record);
        }

        primary_records.sort_by_key(|r| best_authority_rank(&r.capsule));

        let mut resolved: Vec<ResolvedCapsule> = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut gaps: Vec<KnownGap> = Vec::new();

        for record in primary_records {
            let ceiling = if request.include_dependencies {
                self.effective_ceiling(&record, &mut gaps, &mut HashSet::new())?
            } else {
                record.capsule.confidence.ceiling
            };

            if !seen.contains_key(&record.logical_id) {
                seen.insert(record.logical_id.clone(), resolved.len());
                resolved.push(ResolvedCapsule::from((record.clone(), IncludedAs::Primary, ceiling)));
            }

            if request.include_dependencies {
                self.collect_dependencies(&record, &mut resolved, &mut seen, &mut gaps, &mut HashSet::new())?;
            }
        }

        let mut contradictions = Vec::new();
        if request.include_contradictions {
            let snapshot: Vec<(String, String, Capsule)> =
                resolved.iter().map(|r| (r.logical_id.clone(), r.version.clone(), r.capsule.clone())).collect();
            for (logical_id, version, capsule) in snapshot {
                for contradiction in &capsule.contradictions {
                    let (target_logical, target_version) = split_pinned(&contradiction.capsule_id);
                    let target_record = match target_version {
                        Some(v) => self.registry.get(&target_logical, &v)?,
                        None => self.registry.latest_published(&target_logical, request.as_of)?,
                    };
                    let target_resolved = target_record.is_some();
                    if let Some(target) = target_record {
                        if !seen.contains_key(&target.logical_id) {
                            seen.insert(target.logical_id.clone(), resolved.len());
                            let ceiling = target.capsule.confidence.ceiling;
                            resolved.push(ResolvedCapsule::from((target, IncludedAs::Contradiction, ceiling)));
                        }
                    }
                    contradictions.push(ContradictionBundleEntry {
                        declaring_logical_id: logical_id.clone(),
                        declaring_version: version.clone(),
                        target_capsule_id: contradiction.capsule_id.clone(),
                        target_resolved,
                        description: contradiction.description.clone(),
                    });
                }
            }
        }

        Ok(ResolutionBundle { request: request.clone(), capsules: resolved, contradictions, gaps, plan })
    }

    /// Re-runs a previously produced bundle's request against the
    /// registry's *current* state — "replayable resolution."
    pub fn replay(&self, bundle: &ResolutionBundle) -> Result<ResolutionBundle, ResolverError> {
        self.resolve(&bundle.request)
    }

    fn effective_ceiling(&self, record: &CapsuleRecord, gaps: &mut Vec<KnownGap>, visited: &mut HashSet<String>) -> Result<f64, ResolverError> {
        let key = format!("{}@{}", record.logical_id, record.version);
        if !visited.insert(key) {
            return Ok(record.capsule.confidence.ceiling);
        }
        let mut ceiling = record.capsule.confidence.ceiling;
        for dep in self.registry.dependencies_of(&record.logical_id, &record.version)? {
            if dep.relation != DependencyRelation::Requires {
                continue;
            }
            if let Some(target) = self.resolve_dependency_target(&dep, gaps)? {
                ceiling = ceiling.min(self.effective_ceiling(&target, gaps, visited)?);
            }
        }
        Ok(ceiling)
    }

    fn resolve_dependency_target(&self, dep: &DependencyRecord, gaps: &mut Vec<KnownGap>) -> Result<Option<CapsuleRecord>, ResolverError> {
        let target = match &dep.depends_on_version {
            Some(v) => self.registry.get(&dep.depends_on_logical_id, v)?,
            None => self.registry.latest_published(&dep.depends_on_logical_id, None)?,
        };
        if target.is_none() {
            gaps.push(KnownGap {
                logical_id: dep.depends_on_logical_id.clone(),
                version: dep.depends_on_version.clone(),
                reason: "required dependency not found or not published".to_string(),
            });
        }
        Ok(target)
    }

    fn collect_dependencies(
        &self,
        record: &CapsuleRecord,
        resolved: &mut Vec<ResolvedCapsule>,
        seen: &mut HashMap<String, usize>,
        gaps: &mut Vec<KnownGap>,
        visited: &mut HashSet<String>,
    ) -> Result<(), ResolverError> {
        let key = format!("{}@{}", record.logical_id, record.version);
        if !visited.insert(key) {
            return Ok(());
        }
        for dep in self.registry.dependencies_of(&record.logical_id, &record.version)? {
            if dep.relation != DependencyRelation::Requires {
                continue;
            }
            if let Some(target) = self.resolve_dependency_target(&dep, gaps)? {
                if !seen.contains_key(&target.logical_id) {
                    seen.insert(target.logical_id.clone(), resolved.len());
                    let ceiling = self.effective_ceiling(&target, gaps, &mut HashSet::new())?;
                    resolved.push(ResolvedCapsule::from((target.clone(), IncludedAs::Dependency, ceiling)));
                }
                self.collect_dependencies(&target, resolved, seen, gaps, visited)?;
            }
        }
        Ok(())
    }
}

fn exclude(record: &CapsuleRecord, reason: &str) -> PlanStep {
    PlanStep { logical_id: record.logical_id.clone(), version: Some(record.version.clone()), decision: Decision::Excluded, reason: reason.to_string() }
}

fn best_authority_rank(capsule: &Capsule) -> u8 {
    capsule.evidence.iter().map(|e| authority_rank(e.authority)).min().unwrap_or(4)
}

fn authority_rank(a: EvidenceAuthority) -> u8 {
    match a {
        EvidenceAuthority::Primary => 0,
        EvidenceAuthority::Secondary => 1,
        EvidenceAuthority::Derived => 2,
        EvidenceAuthority::Unverified => 3,
    }
}

fn split_pinned(capsule_id: &str) -> (String, Option<String>) {
    match capsule_id.rsplit_once('@') {
        Some((logical, version)) => (logical.to_string(), Some(version.to_string())),
        None => (capsule_id.to_string(), None),
    }
}
