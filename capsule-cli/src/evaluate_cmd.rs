use anyhow::{bail, Context, Result};
use capsule_evaluator::anthropic::AnthropicJudge;
use capsule_evaluator::evaluate::{DecidedBy, Support};
use capsule_evaluator::judge::{Judge, NoJudge};
use capsule_evaluator::surface::JudgeDiscretion;
use capsule_evaluator::{evaluate, CapsulePack, SurfaceManifest, Verdict};
use clap::{Args, ValueEnum};
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
pub enum JudgeArg {
    /// Residual questions stay unresolved.
    None,
    /// Claude via the Messages API (ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN).
    Anthropic,
}

#[derive(Args)]
pub struct EvaluateArgs {
    /// Answer text file, or `-` for stdin.
    #[arg(long)]
    pub answer: PathBuf,
    /// Capsule JSON files or directories of them.
    #[arg(long, required = true, num_args = 1..)]
    pub capsules: Vec<PathBuf>,
    /// Evaluation-surface manifest (YAML). Default: every check, no judge.
    #[arg(long)]
    pub surface: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "none")]
    pub judge: JudgeArg,
    /// Print the full report as JSON.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: EvaluateArgs) -> Result<()> {
    let answer = if args.answer.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(&args.answer).with_context(|| format!("reading {}", args.answer.display()))?
    };
    let pack = CapsulePack::load(&args.capsules)?;
    let surface = match &args.surface {
        Some(p) => SurfaceManifest::from_yaml(&std::fs::read_to_string(p)?).map_err(anyhow::Error::msg)?,
        None => SurfaceManifest::default(),
    };
    let judge: Box<dyn Judge> = match args.judge {
        JudgeArg::None => Box::new(NoJudge),
        JudgeArg::Anthropic => {
            if surface.judge.discretion == JudgeDiscretion::None {
                bail!("the surface manifest gives the judge no discretion; set judge.discretion and judge.may_answer");
            }
            Box::new(AnthropicJudge::from_env().context("set ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN")?)
        }
    };
    let report = evaluate(&answer, &pack, &surface, judge.as_ref());
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("verdict: {:?}   (surface {}, judge {})", report.verdict, report.surface_manifest, report.judge);
        println!(
            "scores:  surface reduction {:.2}   epistemic fidelity {:.2}   laundering risk {:.2}",
            report.scores.evaluation_surface_reduction, report.scores.epistemic_fidelity, report.scores.confidence_laundering_risk
        );
        println!("\nclaims:");
        for c in &report.claims {
            let s = match c.support {
                Support::Supported => "supported",
                Support::Unsupported => "UNSUPPORTED",
                Support::Unresolved => "unresolved",
                Support::Qualifying => "caveat",
            };
            let cap = c.capsule.as_ref().map(|m| format!(" <- {} ({:.2})", m.capsule, m.score)).unwrap_or_default();
            println!("  [{}] {s:<11} {}{cap}", c.claim.index, c.claim.text);
        }
        if !report.findings.is_empty() {
            println!("\nfindings:");
            for f in &report.findings {
                let at = f.claim.map(|i| format!("claim {i}: ")).unwrap_or_default();
                println!("  {:?} {:?}  {at}{}", f.severity, f.kind, f.message);
            }
        }
        let judged = report.decisions.iter().filter(|d| d.decided_by == DecidedBy::Judge).count();
        let open = report.decisions.iter().filter(|d| d.decided_by == DecidedBy::Unresolved).count();
        println!("\ndecisions: {} total, {judged} by judge, {open} unresolved", report.decisions.len());
    }
    if report.verdict == Verdict::Fail {
        std::process::exit(2);
    }
    Ok(())
}
