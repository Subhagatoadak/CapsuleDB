//! `capsule bench` (Phase 10): measures resolution latency and
//! compilation cost on *this* machine, against *this* registry/source —
//! deliberately not a fixed number claimed in documentation, since that
//! would be a hardware-dependent claim this environment has no way to
//! honestly generalize. What's real and reusable is the measurement
//! tool itself.

use crate::compile_cmd::SourceFormatArg;
use anyhow::{bail, Context, Result};
use capsule_compiler::{compile, ingest_html, ingest_markdown, ingest_pdf, ingest_text, CompileOptions, Source};
use capsule_registry::Registry;
use capsule_resolver::{ResolutionRequest, Resolver};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

struct LatencyStats {
    iterations: u32,
    min: Duration,
    median: Duration,
    mean: Duration,
    p95: Duration,
    max: Duration,
}

impl std::fmt::Display for LatencyStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "n={} min={:.3}ms median={:.3}ms mean={:.3}ms p95={:.3}ms max={:.3}ms",
            self.iterations,
            self.min.as_secs_f64() * 1000.0,
            self.median.as_secs_f64() * 1000.0,
            self.mean.as_secs_f64() * 1000.0,
            self.p95.as_secs_f64() * 1000.0,
            self.max.as_secs_f64() * 1000.0,
        )
    }
}

fn measure(iterations: u32, mut f: impl FnMut()) -> LatencyStats {
    // One untimed warm-up call so page faults / first-call allocator
    // overhead don't skew the very first measured sample.
    f();

    let mut samples: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    samples.sort();

    let total: Duration = samples.iter().sum();
    let mean = total / iterations.max(1);
    let median = samples[samples.len() / 2];
    let p95_index = ((samples.len() as f64) * 0.95) as usize;
    let p95 = samples[p95_index.min(samples.len() - 1)];

    LatencyStats { iterations, min: samples[0], median, mean, p95, max: *samples.last().unwrap() }
}

#[allow(clippy::too_many_arguments)]
pub fn run_resolve(
    db: &Path,
    purpose: &str,
    domain: Option<String>,
    about: Option<String>,
    jurisdiction: Option<String>,
    consumer_role: Option<String>,
    iterations: u32,
) -> Result<()> {
    let registry = Registry::open(db).with_context(|| format!("opening registry at {}", db.display()))?;
    let resolver = Resolver::new(&registry);

    let mut request = ResolutionRequest::new(purpose);
    request.domain = domain;
    request.about = about;
    request.jurisdiction = jurisdiction;
    request.consumer_role = consumer_role;

    let stats = measure(iterations, || {
        let _ = resolver.resolve(&request);
    });
    println!("resolve latency: {stats}");
    Ok(())
}

/// Times `POST /resolve` against a *running* `capsule-api` server over a
/// real TCP connection — the "remote" half of "benchmark local and
/// remote resolution latency," as opposed to `run_resolve`'s in-process
/// call. A bare HTTP/1.1 client (no crate dependency), matching
/// `capsule-api`'s own `tiny_http`-level minimalism rather than pulling
/// in a full HTTP client for one benchmark command.
pub fn run_remote_resolve(addr: &str, purpose: &str, iterations: u32) -> Result<()> {
    let request = ResolutionRequest::new(purpose);
    let body = serde_json::to_vec(&request)?;

    let stats = measure(iterations, || {
        let _ = post_resolve(addr, &body);
    });
    println!("remote resolve latency (POST /resolve @ {addr}): {stats}");
    Ok(())
}

fn post_resolve(addr: &str, body: &[u8]) -> Result<()> {
    let mut stream = TcpStream::connect(addr).with_context(|| format!("connecting to {addr}"))?;
    let request = format!(
        "POST /resolve HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    if response.is_empty() {
        bail!("empty response from {addr}");
    }
    Ok(())
}

pub fn run_compile(source_path: &Path, format: Option<SourceFormatArg>, domain: &str, topic: &str, owner: &str, iterations: u32) -> Result<()> {
    let bytes = std::fs::read(source_path).with_context(|| format!("reading {}", source_path.display()))?;
    let format = format.unwrap_or_else(|| infer_format(source_path));
    let source: Source = match format {
        SourceFormatArg::Text => ingest_text(&bytes),
        SourceFormatArg::Markdown => ingest_markdown(&bytes),
        SourceFormatArg::Html => ingest_html(&bytes),
        SourceFormatArg::Pdf => ingest_pdf(&bytes),
    }
    .context("ingesting source")?;

    let options = CompileOptions { domain: domain.to_string(), topic: topic.to_string(), owner: owner.to_string() };

    let stats = measure(iterations, || {
        let _ = compile(&source, &options);
    });
    println!("compile latency: {stats}");
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
