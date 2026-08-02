use anyhow::{bail, Context, Result};
use capsule_compiler::{compile, ingest_html, ingest_markdown, ingest_pdf, ingest_text, CompileOptions, Source};
use capsule_core::Severity;
use clap::ValueEnum;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, ValueEnum)]
pub enum SourceFormatArg {
    Text,
    Markdown,
    Html,
    Pdf,
}

#[allow(clippy::too_many_arguments)]
pub fn run(source_path: &Path, format: Option<SourceFormatArg>, domain: &str, topic: &str, owner: &str, out_dir: &Path) -> Result<()> {
    let bytes = fs::read(source_path).with_context(|| format!("reading {}", source_path.display()))?;
    let format = format.unwrap_or_else(|| infer_format(source_path));

    let source: Source = match format {
        SourceFormatArg::Text => ingest_text(&bytes),
        SourceFormatArg::Markdown => ingest_markdown(&bytes),
        SourceFormatArg::Html => ingest_html(&bytes),
        SourceFormatArg::Pdf => ingest_pdf(&bytes),
    }
    .context("ingesting source")?;

    let options = CompileOptions { domain: domain.to_string(), topic: topic.to_string(), owner: owner.to_string() };
    let report = compile(&source, &options);

    if report.candidates.is_empty() {
        bail!("no candidate claims were extracted from {}", source_path.display());
    }

    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    println!("source_id: {}", report.source_id);
    println!("{} candidate(s), {} duplicate(s) skipped\n", report.candidates.len(), report.duplicates.len());

    for (i, candidate) in report.candidates.iter().enumerate() {
        let file_name = format!("{:03}.json", i + 1);
        let path = out_dir.join(&file_name);
        let json = serde_json::to_string_pretty(&candidate.capsule)?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;

        println!("[{}] {}", i + 1, candidate.capsule.logical_id);
        println!("    statement       : {}", candidate.capsule.statement);
        println!("    epistemic_status: {:?}", candidate.capsule.epistemic_status);
        println!("    written_to      : {}", path.display());

        if !candidate.qualifications.is_empty() {
            println!("    qualifications  : {:?}", candidate.qualifications);
        }
        if !candidate.exclusions.is_empty() {
            println!("    exclusions      : {:?}", candidate.exclusions);
        }
        if !candidate.applicability_conditions.is_empty() {
            println!("    conditions      : {:?}", candidate.applicability_conditions);
        }
        if !candidate.referenced_capsules.is_empty() {
            println!("    references      : {:?} (not auto-wired as dependencies; review and add manually)", candidate.referenced_capsules);
        }
        for issue in &candidate.validation_issues {
            let label = match issue.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARN ",
            };
            println!("    {label} [{}] {}", issue.code, issue.message);
        }
        println!();
    }

    if !report.duplicates.is_empty() {
        println!("duplicate evidence (identical text seen more than once in this source):");
        for dup in &report.duplicates {
            println!("  - \"{}\" (first seen as candidate #{})", dup.text, dup.first_candidate_index + 1);
        }
    }

    Ok(())
}

fn infer_format(path: &Path) -> SourceFormatArg {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
        Some(ext) if ext == "md" || ext == "markdown" => SourceFormatArg::Markdown,
        Some(ext) if ext == "html" || ext == "htm" => SourceFormatArg::Html,
        Some(ext) if ext == "pdf" => SourceFormatArg::Pdf,
        _ => SourceFormatArg::Text,
    }
}
