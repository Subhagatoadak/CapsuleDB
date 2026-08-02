use anyhow::{Context, Result};
use capsule_registry::Registry;
use capsule_resolver::{Decision, IncludedAs, ResolutionRequest, Resolver};
use chrono::NaiveDate;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub fn run(
    db: &Path,
    purpose: &str,
    domain: Option<String>,
    about: Option<String>,
    jurisdiction: Option<String>,
    as_of: Option<String>,
    consumer_role: Option<String>,
    require_sufficient_evidence: bool,
    show_plan: bool,
) -> Result<()> {
    let registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;
    let resolver = Resolver::new(&registry);

    let mut request = ResolutionRequest::new(purpose);
    request.domain = domain;
    request.about = about;
    request.jurisdiction = jurisdiction;
    request.consumer_role = consumer_role;
    request.require_sufficient_evidence = require_sufficient_evidence;
    if let Some(date) = as_of {
        request.as_of = Some(NaiveDate::parse_from_str(&date, "%Y-%m-%d").with_context(|| format!("parsing --as-of '{date}' as YYYY-MM-DD"))?);
    }

    let bundle = resolver.resolve(&request)?;

    println!("resolved {} capsule(s) for purpose \"{}\"\n", bundle.capsules.len(), bundle.request.purpose);
    for resolved in &bundle.capsules {
        let role = match resolved.included_as {
            IncludedAs::Primary => "primary",
            IncludedAs::Dependency => "dependency",
            IncludedAs::Contradiction => "contradiction",
        };
        println!(
            "[{role}] {}@{} (ceiling={:.2}) {:?}",
            resolved.logical_id, resolved.version, resolved.effective_confidence_ceiling, resolved.lifecycle_state
        );
        println!("    {}", resolved.capsule.statement);
    }

    if !bundle.contradictions.is_empty() {
        println!("\ncontradictions:");
        for c in &bundle.contradictions {
            let resolved_note = if c.target_resolved { "" } else { " (target not in registry)" };
            println!("  {}@{} vs {}{}: {}", c.declaring_logical_id, c.declaring_version, c.target_capsule_id, resolved_note, c.description);
        }
    }

    if !bundle.gaps.is_empty() {
        println!("\nknown gaps:");
        for gap in &bundle.gaps {
            let version = gap.version.as_deref().unwrap_or("*");
            println!("  {}@{}: {}", gap.logical_id, version, gap.reason);
        }
    }

    if show_plan {
        println!("\nresolution plan:");
        for step in &bundle.plan {
            let version = step.version.as_deref().unwrap_or("*");
            let mark = match step.decision {
                Decision::Included => "+",
                Decision::Excluded => "-",
            };
            println!("  {mark} {}@{}: {}", step.logical_id, version, step.reason);
        }
    }

    Ok(())
}
