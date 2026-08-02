use anyhow::{Context, Result};
use capsule_observatory::{run, ObservatoryOptions};
use capsule_registry::Registry;
use std::path::Path;

pub fn run_cmd(db: &Path, freshness_days: i64, knowledge_debt_days: i64) -> Result<()> {
    let registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;
    let options = ObservatoryOptions { freshness_threshold_days: freshness_days, knowledge_debt_threshold_days: knowledge_debt_days };
    let report = run(&registry, &options)?;

    println!("scanned {} capsule version(s)", report.total_capsules_scanned);
    println!("capsule_reuse_ratio: {:.2}", report.capsule_reuse_ratio);
    println!();

    print_section("unresolved contradictions", report.unresolved_contradictions.len(), || {
        report.unresolved_contradictions.iter().map(|c| format!("{}@{} vs {}: {}", c.logical_id, c.version, c.target_capsule_id, c.description)).collect()
    });
    print_section("broken dependencies", report.broken_dependencies.len(), || {
        report
            .broken_dependencies
            .iter()
            .map(|d| format!("{}@{} requires missing {}{}", d.logical_id, d.version, d.depends_on_logical_id, d.depends_on_version.as_ref().map(|v| format!("@{v}")).unwrap_or_default()))
            .collect()
    });
    print_section("confidence-laundering incidents", report.confidence_laundering_incidents.len(), || {
        report
            .confidence_laundering_incidents
            .iter()
            .map(|c| format!("{}@{}: '{}' -> '{}' ({})", c.logical_id, c.version, c.weak_phrase, c.inflated_phrase, c.pattern))
            .collect()
    });
    print_section("expired but still published", report.expiry_alerts.len(), || {
        report.expiry_alerts.iter().map(|e| format!("{}@{}: valid_until {} has passed", e.logical_id, e.version, e.valid_until)).collect()
    });
    print_section("knowledge debt (stale unpublished drafts)", report.knowledge_debt.len(), || {
        report.knowledge_debt.iter().map(|k| format!("{}@{} [{:?}] {} day(s) old", k.logical_id, k.version, k.lifecycle_state, k.days_since_recorded)).collect()
    });
    print_section("freshness alerts (stale published capsules)", report.freshness_alerts.len(), || {
        report.freshness_alerts.iter().map(|f| format!("{}@{}: {} day(s) since last change", f.logical_id, f.version, f.days_since_recorded)).collect()
    });
    print_section("orphaned capsules (published, zero dependents)", report.orphaned_capsules.len(), || {
        report.orphaned_capsules.iter().map(|o| format!("{}@{}", o.logical_id, o.version)).collect()
    });

    Ok(())
}

fn print_section(title: &str, count: usize, lines: impl FnOnce() -> Vec<String>) {
    println!("{title} ({count}):");
    if count == 0 {
        println!("  none");
    } else {
        for line in lines() {
            println!("  - {line}");
        }
    }
    println!();
}
