mod registry_cmd;

use anyhow::{bail, Context, Result};
use capsule_core::{content_hash, is_valid, parse, validate, Capsule, Severity};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use registry_cmd::RegistryCommand;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "capsule", version, about = "Inspect, validate, pack, and unpack Knowledge Capsules")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pretty-print a capsule's epistemic fields.
    Inspect {
        /// Path to a capsule JSON document.
        capsule: PathBuf,
    },
    /// Run structural and epistemic validation against a capsule.
    Validate {
        /// Path to a capsule JSON document.
        capsule: PathBuf,
    },
    /// Package a capsule (and optional evidence files) into a .kcap archive.
    Pack {
        /// Path to a capsule JSON document.
        capsule: PathBuf,
        /// Output .kcap path.
        #[arg(short, long)]
        output: PathBuf,
        /// Optional directory of evidence source files to bundle under evidence/.
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
    },
    /// Extract a .kcap archive, verifying manifest checksums and content hash.
    Unpack {
        /// Path to a .kcap archive.
        package: PathBuf,
        /// Output directory.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Register, transition, publish, and inspect capsules in a
    /// SQLite-backed capsule-registry.
    Registry {
        /// Path to the registry's SQLite database (created if missing).
        #[arg(long, default_value = "capsuledb.sqlite3", global = true)]
        db: PathBuf,
        #[command(subcommand)]
        action: RegistryCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { capsule } => cmd_inspect(&capsule),
        Command::Validate { capsule } => cmd_validate(&capsule),
        Command::Pack { capsule, output, evidence_dir } => cmd_pack(&capsule, &output, evidence_dir.as_deref()),
        Command::Unpack { package, output } => cmd_unpack(&package, &output),
        Command::Registry { db, action } => registry_cmd::run(&db, action),
    }
}

pub(crate) fn load_capsule(path: &Path) -> Result<Capsule> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing capsule at {}", path.display()))
}

fn cmd_inspect(path: &Path) -> Result<()> {
    let capsule = load_capsule(path)?;

    println!("logical_id      : {}", capsule.logical_id);
    println!("version         : {}", capsule.version);
    println!("capsule_type    : {:?}", capsule.capsule_type);
    println!("lifecycle_state : {:?}", capsule.lifecycle_state);
    println!("epistemic_status: {:?}", capsule.epistemic_status);
    println!(
        "confidence      : basis={:?} ceiling={:.2}",
        capsule.confidence.basis, capsule.confidence.ceiling
    );
    println!("statement       : {}", capsule.statement);

    println!("evidence        : {} entr{}", capsule.evidence.len(), if capsule.evidence.len() == 1 { "y" } else { "ies" });
    for e in &capsule.evidence {
        println!("  - {} [{:?}]", e.source_id, e.authority);
    }

    if let Some(app) = &capsule.applicability {
        println!(
            "applicability   : jurisdictions={:?} valid_from={:?} valid_until={:?}",
            app.jurisdictions, app.valid_from, app.valid_until
        );
    }

    if !capsule.contradictions.is_empty() {
        println!("contradictions  :");
        for c in &capsule.contradictions {
            println!("  - [{:?}] vs {}: {}", c.resolution_status, c.capsule_id, c.description);
        }
    }

    if !capsule.dependencies.is_empty() {
        println!("dependencies    :");
        for d in &capsule.dependencies {
            println!("  - {:?} {}", d.relation, d.capsule_id);
        }
    }

    println!("governance      : owner={} domain={}", capsule.governance.owner, capsule.governance.domain);

    let computed = content_hash(&capsule)?;
    match &capsule.content_hash {
        Some(declared) if declared == &computed => println!("content_hash    : {declared} (verified)"),
        Some(declared) => println!("content_hash    : {declared} (MISMATCH, computed {computed})"),
        None => println!("content_hash    : (unset, computed {computed})"),
    }

    Ok(())
}

fn cmd_validate(path: &Path) -> Result<()> {
    let capsule = load_capsule(path)?;
    let issues = validate(&capsule);

    if issues.is_empty() {
        println!("OK  {} is a conformant KCP 0.1 capsule", capsule.logical_id);
        return Ok(());
    }

    for issue in &issues {
        let label = match issue.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
        };
        println!("{label} [{}] {}", issue.code, issue.message);
    }

    if is_valid(&capsule) {
        println!("\nOK  {} conforms (with warnings above)", capsule.logical_id);
        Ok(())
    } else {
        bail!("{} failed validation", capsule.logical_id);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn cmd_pack(capsule_path: &Path, output: &Path, evidence_dir: Option<&Path>) -> Result<()> {
    let capsule_text = fs::read_to_string(capsule_path).with_context(|| format!("reading {}", capsule_path.display()))?;
    let capsule = parse(&capsule_text).with_context(|| format!("parsing capsule at {}", capsule_path.display()))?;

    if !is_valid(&capsule) {
        for issue in validate(&capsule).into_iter().filter(|i| i.severity == Severity::Error) {
            eprintln!("ERROR [{}] {}", issue.code, issue.message);
        }
        bail!("refusing to pack an invalid capsule (use `capsule validate` for details)");
    }

    let capsule_json = serde_json::to_vec_pretty(&capsule)?;
    let hash = content_hash(&capsule)?;

    let mut files = vec![ManifestFile { path: "capsule.json".to_string(), sha256: sha256_hex(&capsule_json) }];
    let mut evidence_files: Vec<(String, Vec<u8>)> = Vec::new();

    if let Some(dir) = evidence_dir {
        for entry in fs::read_dir(dir).with_context(|| format!("reading evidence dir {}", dir.display()))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let bytes = fs::read(entry.path())?;
            let archive_path = format!("evidence/{}", entry.file_name().to_string_lossy());
            files.push(ManifestFile { path: archive_path.clone(), sha256: sha256_hex(&bytes) });
            evidence_files.push((archive_path, bytes));
        }
    }

    let manifest = Manifest {
        kcp_version: "0.1".to_string(),
        packaged_at: chrono::Utc::now().to_rfc3339(),
        capsule_logical_id: capsule.logical_id.clone(),
        capsule_version: capsule.version.clone(),
        content_hash: hash,
        files,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;

    let output_file = fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let encoder = GzEncoder::new(output_file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    append_bytes(&mut builder, "manifest.json", &manifest_json)?;
    append_bytes(&mut builder, "capsule.json", &capsule_json)?;
    for (path, bytes) in &evidence_files {
        append_bytes(&mut builder, path, bytes)?;
    }

    builder.finish()?;
    println!("packed {} -> {}", capsule.logical_id, output.display());
    Ok(())
}

fn append_bytes<W: Write>(builder: &mut tar::Builder<W>, path: &str, bytes: &[u8]) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes)?;
    Ok(())
}

fn cmd_unpack(package: &Path, output: &Path) -> Result<()> {
    let file = fs::File::open(package).with_context(|| format!("opening {}", package.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    fs::create_dir_all(output)?;

    let mut extracted: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;

        let dest = output.join(&path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &bytes)?;
        extracted.push((path, bytes));
    }

    let manifest_bytes = extracted
        .iter()
        .find(|(path, _)| path == "manifest.json")
        .map(|(_, bytes)| bytes.clone())
        .context("archive is missing manifest.json")?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).context("parsing manifest.json")?;

    for file in &manifest.files {
        let (_, bytes) = extracted
            .iter()
            .find(|(path, _)| path == &file.path)
            .with_context(|| format!("manifest references missing file '{}'", file.path))?;
        let actual = sha256_hex(bytes);
        if actual != file.sha256 {
            bail!("checksum mismatch for '{}': manifest says {}, actual {}", file.path, file.sha256, actual);
        }
    }

    let capsule_bytes = extracted
        .iter()
        .find(|(path, _)| path == "capsule.json")
        .map(|(_, bytes)| bytes.clone())
        .context("archive is missing capsule.json")?;
    let capsule: Capsule = serde_json::from_slice(&capsule_bytes).context("parsing capsule.json")?;
    let computed = content_hash(&capsule)?;
    if computed != manifest.content_hash {
        bail!(
            "content hash mismatch: manifest declares {}, recomputed {}",
            manifest.content_hash,
            computed
        );
    }

    println!(
        "unpacked {} ({} files) -> {}, content hash verified",
        manifest.capsule_logical_id,
        manifest.files.len(),
        output.display()
    );
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    kcp_version: String,
    packaged_at: String,
    capsule_logical_id: String,
    capsule_version: String,
    content_hash: String,
    files: Vec<ManifestFile>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ManifestFile {
    path: String,
    sha256: String,
}
