use anyhow::{Context, Result};
use capsule_evaluator::{evaluate, DiscretionLevel, EvaluateOptions, NullJudge};
use capsule_registry::Registry;
use clap::ValueEnum;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, ValueEnum)]
pub enum DiscretionArg {
    None,
    Low,
    Medium,
    High,
}

impl From<DiscretionArg> for DiscretionLevel {
    fn from(value: DiscretionArg) -> Self {
        match value {
            DiscretionArg::None => DiscretionLevel::None,
            DiscretionArg::Low => DiscretionLevel::Low,
            DiscretionArg::Medium => DiscretionLevel::Medium,
            DiscretionArg::High => DiscretionLevel::High,
        }
    }
}

pub fn run(db: &Path, answer_file: &Path, discretion: DiscretionArg) -> Result<()> {
    let answer = fs::read_to_string(answer_file).with_context(|| format!("reading {}", answer_file.display()))?;
    let registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;

    let options = EvaluateOptions { discretion: discretion.into(), ..Default::default() };
    let report = evaluate(&answer, &registry, &NullJudge, &options)?;

    println!("claims: {}", report.surface.total_claims);
    println!("  strongly supported : {}", report.surface.claims_strongly_supported);
    println!("  referred to judge  : {}", report.surface.claims_ambiguous_referred_to_judge);
    println!("  unsupported        : {}", report.surface.claims_unsupported);
    println!();
    println!("evaluation_surface_reduction: {:.2}", report.scores.evaluation_surface_reduction);
    println!("epistemic_fidelity          : {:.2}", report.scores.epistemic_fidelity);
    println!("confidence_laundering_risk  : {:.2}", report.scores.confidence_laundering_risk);

    if !report.findings.is_empty() {
        println!("\nfindings:");
        for finding in &report.findings {
            let claim = finding.claim_index.map(|i| format!("claim #{i}")).unwrap_or_else(|| "-".to_string());
            println!("  [{:?}] {claim}: {}", finding.kind, finding.message);
        }
    }

    if !report.judge_verdicts.is_empty() {
        println!("\njudge verdicts (NullJudge always abstains — no LLM is wired in):");
        for (claim_index, verdict) in &report.judge_verdicts {
            println!("  claim #{claim_index}: {verdict:?}");
        }
    }

    Ok(())
}
