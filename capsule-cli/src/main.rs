mod bench_cmd;
mod compile_cmd;
mod crypto;
mod evaluate_cmd;
mod observatory_cmd;
mod pack_list_cmd;
mod registry_cmd;
mod resolve_cmd;

use anyhow::{bail, Context, Result};
use capsule_core::{content_hash, is_valid, parse, validate, Capsule, Severity};
use clap::{Parser, Subcommand};
use compile_cmd::SourceFormatArg;
use evaluate_cmd::DiscretionArg;
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
        /// Path to a hex-encoded Ed25519 secret key (see `capsule keygen`) to sign the package's content hash.
        #[arg(long)]
        sign: Option<PathBuf>,
    },
    /// Extract a .kcap archive, verifying manifest checksums and content hash.
    Unpack {
        /// Path to a .kcap archive.
        package: PathBuf,
        /// Output directory.
        #[arg(short, long)]
        output: PathBuf,
        /// Path to a hex-encoded Ed25519 public key; if given, the package's signature must verify against it.
        #[arg(long)]
        verify_key: Option<PathBuf>,
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
    /// Compile a source document into candidate draft capsules (deterministic, no LLM step).
    Compile {
        /// Path to the source document.
        source: PathBuf,
        /// Source format; inferred from the file extension if omitted.
        #[arg(long, value_enum)]
        format: Option<SourceFormatArg>,
        #[arg(long)]
        domain: String,
        #[arg(long)]
        topic: String,
        #[arg(long)]
        owner: String,
        /// Directory to write one JSON draft capsule per extracted claim.
        #[arg(long, default_value = "draft-capsules")]
        out_dir: PathBuf,
    },
    /// Resolve the applicable capsule set for a request against a registry.
    Resolve {
        /// Path to the registry's SQLite database.
        #[arg(long, default_value = "capsuledb.sqlite3")]
        db: PathBuf,
        purpose: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        about: Option<String>,
        #[arg(long)]
        jurisdiction: Option<String>,
        /// Resolve as of this date (YYYY-MM-DD); defaults to today.
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        consumer_role: Option<String>,
        /// Exclude candidates whose evidence is entirely unverified.
        #[arg(long)]
        require_sufficient_evidence: bool,
        /// Print the full inclusion/exclusion decision plan.
        #[arg(long)]
        plan: bool,
    },
    /// Generate an Ed25519 keypair for signing capsule content hashes.
    Keygen {
        /// Write "<prefix>.pub" (hex public key) and "<prefix>.key" (hex secret key) instead of printing to stdout.
        #[arg(long)]
        out_prefix: Option<PathBuf>,
    },
    /// Evaluate an answer against the capsules that back it.
    Evaluate {
        /// Path to a text file containing the answer to evaluate.
        answer_file: PathBuf,
        #[arg(long, default_value = "capsuledb.sqlite3")]
        db: PathBuf,
        /// How wide an ambiguous-support band gets referred to the judge (a no-op judge in this build).
        #[arg(long, value_enum, default_value = "low")]
        discretion: DiscretionArg,
    },
    /// Run registry-wide health checks: freshness, contradictions, orphans, broken dependencies, and more.
    Observatory {
        #[arg(long, default_value = "capsuledb.sqlite3")]
        db: PathBuf,
        #[arg(long, default_value_t = 180)]
        freshness_days: i64,
        #[arg(long, default_value_t = 30)]
        knowledge_debt_days: i64,
    },
    /// Create a Capsule Pack manifest: a named, versioned bundle of capsule references.
    PackCreate {
        output: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Members as `kc://domain/topic/name` (latest published) or `kc://domain/topic/name@1.2.3` (pinned).
        members: Vec<String>,
    },
    /// Resolve a Capsule Pack's members against a registry and confirm they're all published.
    PackResolve {
        pack_file: PathBuf,
        #[arg(long, default_value = "capsuledb.sqlite3")]
        db: PathBuf,
    },
    /// Generate a random AES-256 key for `registry backup --encrypt-key-file`.
    Datakey {
        /// Write the hex key to this file instead of printing to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Measure resolution latency or compilation cost on this machine (no fixed numbers claimed anywhere else).
    Bench {
        #[command(subcommand)]
        action: BenchCommand,
    },
}

#[derive(Subcommand)]
enum BenchCommand {
    /// Time `capsule-resolver`'s resolve() against a registry.
    Resolve {
        #[arg(long, default_value = "capsuledb.sqlite3")]
        db: PathBuf,
        purpose: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        about: Option<String>,
        #[arg(long)]
        jurisdiction: Option<String>,
        #[arg(long)]
        consumer_role: Option<String>,
        #[arg(long, default_value_t = 100)]
        iterations: u32,
    },
    /// Time `POST /resolve` against a running capsule-api server over real HTTP (the "remote" half of the roadmap's latency benchmark).
    RemoteResolve {
        /// capsule-api address, e.g. 127.0.0.1:8089.
        addr: String,
        purpose: String,
        #[arg(long, default_value_t = 100)]
        iterations: u32,
    },
    /// Time `capsule-compiler`'s compile() against a source document.
    Compile {
        source: PathBuf,
        #[arg(long, value_enum)]
        format: Option<SourceFormatArg>,
        #[arg(long)]
        domain: String,
        #[arg(long)]
        topic: String,
        #[arg(long)]
        owner: String,
        #[arg(long, default_value_t = 100)]
        iterations: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { capsule } => cmd_inspect(&capsule),
        Command::Validate { capsule } => cmd_validate(&capsule),
        Command::Pack { capsule, output, evidence_dir, sign } => cmd_pack(&capsule, &output, evidence_dir.as_deref(), sign.as_deref()),
        Command::Unpack { package, output, verify_key } => cmd_unpack(&package, &output, verify_key.as_deref()),
        Command::Registry { db, action } => registry_cmd::run(&db, action),
        Command::Compile { source, format, domain, topic, owner, out_dir } => compile_cmd::run(&source, format, &domain, &topic, &owner, &out_dir),
        Command::Resolve { db, purpose, domain, about, jurisdiction, as_of, consumer_role, require_sufficient_evidence, plan } => {
            resolve_cmd::run(&db, &purpose, domain, about, jurisdiction, as_of, consumer_role, require_sufficient_evidence, plan)
        }
        Command::Keygen { out_prefix } => cmd_keygen(out_prefix.as_deref()),
        Command::Evaluate { answer_file, db, discretion } => evaluate_cmd::run(&db, &answer_file, discretion),
        Command::Observatory { db, freshness_days, knowledge_debt_days } => observatory_cmd::run_cmd(&db, freshness_days, knowledge_debt_days),
        Command::PackCreate { output, name, description, members } => pack_list_cmd::create(&output, &name, description.as_deref(), members),
        Command::PackResolve { pack_file, db } => pack_list_cmd::resolve(&pack_file, &db),
        Command::Datakey { out } => cmd_datakey(out.as_deref()),
        Command::Bench { action } => match action {
            BenchCommand::Resolve { db, purpose, domain, about, jurisdiction, consumer_role, iterations } => {
                bench_cmd::run_resolve(&db, &purpose, domain, about, jurisdiction, consumer_role, iterations)
            }
            BenchCommand::Compile { source, format, domain, topic, owner, iterations } => {
                bench_cmd::run_compile(&source, format, &domain, &topic, &owner, iterations)
            }
            BenchCommand::RemoteResolve { addr, purpose, iterations } => bench_cmd::run_remote_resolve(&addr, &purpose, iterations),
        },
    }
}

fn cmd_datakey(out: Option<&Path>) -> Result<()> {
    let key = crypto::generate_key_hex();
    match out {
        Some(path) => {
            fs::write(path, &key)?;
            println!("wrote data encryption key -> {}", path.display());
        }
        None => println!("data_key: {key}"),
    }
    Ok(())
}

fn cmd_keygen(out_prefix: Option<&Path>) -> Result<()> {
    let keypair = capsule_runtime::KeyPair::generate().map_err(|e| anyhow::anyhow!(e))?;
    match out_prefix {
        Some(prefix) => {
            let pub_path = append_extension(prefix, "pub");
            let key_path = append_extension(prefix, "key");
            fs::write(&pub_path, keypair.public_key_hex())?;
            fs::write(&key_path, keypair.secret_key_hex())?;
            println!("wrote {} and {}", pub_path.display(), key_path.display());
        }
        None => {
            println!("public_key: {}", keypair.public_key_hex());
            println!("secret_key: {}", keypair.secret_key_hex());
        }
    }
    Ok(())
}

fn append_extension(prefix: &Path, ext: &str) -> PathBuf {
    let mut path = prefix.to_path_buf();
    let file_name = format!("{}.{ext}", path.file_name().and_then(|n| n.to_str()).unwrap_or("capsuledb"));
    path.set_file_name(file_name);
    path
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

fn cmd_pack(capsule_path: &Path, output: &Path, evidence_dir: Option<&Path>, sign: Option<&Path>) -> Result<()> {
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

    let (signature, public_key) = match sign {
        Some(key_path) => {
            let secret_hex = fs::read_to_string(key_path).with_context(|| format!("reading secret key at {}", key_path.display()))?;
            let keypair = capsule_runtime::KeyPair::from_secret_hex(secret_hex.trim()).map_err(|e| anyhow::anyhow!(e))?;
            (Some(keypair.sign_content_hash(&hash)), Some(keypair.public_key_hex()))
        }
        None => (None, None),
    };

    let manifest = Manifest {
        kcp_version: "0.1".to_string(),
        packaged_at: chrono::Utc::now().to_rfc3339(),
        capsule_logical_id: capsule.logical_id.clone(),
        capsule_version: capsule.version.clone(),
        content_hash: hash,
        files,
        signature,
        public_key,
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
    match &manifest.public_key {
        Some(key) => println!("packed {} -> {}, signed by {key}", capsule.logical_id, output.display()),
        None => println!("packed {} -> {}", capsule.logical_id, output.display()),
    }
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

fn cmd_unpack(package: &Path, output: &Path, verify_key: Option<&Path>) -> Result<()> {
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

    let signature_note = match (verify_key, &manifest.signature, &manifest.public_key) {
        (Some(key_path), Some(signature), Some(public_key)) => {
            let public_key_expected = fs::read_to_string(key_path).with_context(|| format!("reading public key at {}", key_path.display()))?;
            let public_key_expected = public_key_expected.trim();
            if public_key_expected != public_key {
                bail!("package is signed by {public_key}, not by the provided verify-key {public_key_expected}");
            }
            capsule_runtime::verify_content_hash(public_key, &manifest.content_hash, signature).map_err(|e| anyhow::anyhow!(e))?;
            format!(", signature verified against {public_key}")
        }
        (Some(_), None, _) => bail!("--verify-key was given but this package is unsigned"),
        (None, Some(_), Some(public_key)) => format!(", signed by {public_key} (pass --verify-key to verify)"),
        _ => String::new(),
    };

    println!(
        "unpacked {} ({} files) -> {}, content hash verified{signature_note}",
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
    /// Hex Ed25519 signature over `content_hash`, if the package was
    /// signed with `capsule pack --sign` (spec: "signed publishers").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    public_key: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ManifestFile {
    path: String,
    sha256: String,
}
