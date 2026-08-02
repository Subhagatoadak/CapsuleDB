use std::fs;
use std::process::Command;

fn capsule_bin() -> &'static str {
    env!("CARGO_BIN_EXE_capsule")
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn validate_accepts_a_conformant_example() {
    let capsule = repo_root().join("examples/capsules/data-retention-90-days.json");
    let output = Command::new(capsule_bin())
        .args(["validate", capsule.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK"));
}

#[test]
fn validate_rejects_a_structurally_invalid_capsule() {
    let dir = tempdir();
    let bad = dir.join("bad.json");
    fs::write(
        &bad,
        r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "NOT-VALID",
          "version": "abc",
          "capsule_type": "fact",
          "statement": "x",
          "epistemic_status": "observed",
          "confidence": {"basis": "unsupported", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "governance": {"owner": "a", "domain": "b"}
        }"#,
    )
    .unwrap();

    let output = Command::new(capsule_bin()).args(["validate", bad.to_str().unwrap()]).output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("logical_id"));
    assert!(stdout.contains("version"));
    assert!(stdout.contains("confidence_ceiling_unsupported"));
}

#[test]
fn pack_then_unpack_round_trips_and_verifies_hash() {
    let dir = tempdir();
    let capsule = repo_root().join("examples/capsules/data-retention-90-days.json");
    let package = dir.join("out.kcap");

    let pack_status = Command::new(capsule_bin())
        .args(["pack", capsule.to_str().unwrap(), "-o", package.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(pack_status.success());
    assert!(package.exists());

    let extract_dir = dir.join("extracted");
    let unpack_output = Command::new(capsule_bin())
        .args(["unpack", package.to_str().unwrap(), "-o", extract_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(unpack_output.status.success(), "{}", String::from_utf8_lossy(&unpack_output.stderr));
    assert!(String::from_utf8_lossy(&unpack_output.stdout).contains("content hash verified"));
    assert!(extract_dir.join("capsule.json").exists());
    assert!(extract_dir.join("manifest.json").exists());
}

#[test]
fn pack_refuses_an_invalid_capsule() {
    let dir = tempdir();
    let bad = dir.join("bad.json");
    fs::write(
        &bad,
        r#"{
          "schema_version": "kcp/0.1",
          "logical_id": "NOT-VALID",
          "version": "1.0.0",
          "capsule_type": "fact",
          "statement": "x",
          "epistemic_status": "observed",
          "confidence": {"basis": "direct_observation", "ceiling": 0.9},
          "lifecycle_state": "draft",
          "governance": {"owner": "a", "domain": "b"}
        }"#,
    )
    .unwrap();
    let package = dir.join("out.kcap");

    let status = Command::new(capsule_bin())
        .args(["pack", bad.to_str().unwrap(), "-o", package.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!package.exists());
}

#[test]
fn registry_register_transition_publish_and_get_round_trip() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();

    let db = dir.join("registry.sqlite3");
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin())
            .arg("registry")
            .arg("--db")
            .arg(&db)
            .args(args)
            .output()
            .unwrap()
    };

    let register = run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]);
    assert!(register.status.success(), "{}", String::from_utf8_lossy(&register.stderr));

    let transition = run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]);
    assert!(transition.status.success(), "{}", String::from_utf8_lossy(&transition.stderr));

    let approve = run(&["transition", &logical_id, "1.0.0", "approved", "--actor", "bob"]);
    assert!(approve.status.success(), "{}", String::from_utf8_lossy(&approve.stderr));

    let publish = run(&["publish", &logical_id, "1.0.0", "--actor", "bob"]);
    assert!(publish.status.success(), "{}", String::from_utf8_lossy(&publish.stderr));

    let get = run(&["get", &logical_id, "1.0.0"]);
    assert!(get.status.success());
    let stdout = String::from_utf8_lossy(&get.stdout);
    assert!(stdout.contains("Published"));

    let skip_state = run(&["transition", &logical_id, "1.0.0", "draft", "--actor", "alice"]);
    assert!(!skip_state.status.success());
}

#[test]
fn compile_extracts_candidates_and_writes_valid_draft_json() {
    let dir = tempdir();
    let source_path = dir.join("policy.txt");
    fs::write(
        &source_path,
        "Data must be retained for 90 days unless a legal hold applies.\n\nData must be retained for 90 days unless a legal hold applies.",
    )
    .unwrap();

    let out_dir = dir.join("drafts");
    let output = Command::new(capsule_bin())
        .args([
            "compile",
            source_path.to_str().unwrap(),
            "--domain",
            "policy",
            "--topic",
            "retention",
            "--owner",
            "alice",
            "--out-dir",
        ])
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 candidate(s), 1 duplicate(s) skipped"));

    let draft_path = out_dir.join("001.json");
    assert!(draft_path.exists());

    let validate = Command::new(capsule_bin()).args(["validate", draft_path.to_str().unwrap()]).output().unwrap();
    assert!(validate.status.success(), "{}", String::from_utf8_lossy(&validate.stderr));
}

#[test]
fn resolve_returns_the_latest_published_matching_capsule() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();

    let db = dir.join("registry.sqlite3");
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let registry_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&db).args(args).output().unwrap()
    };
    assert!(registry_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "approved", "--actor", "alice"]).status.success());
    assert!(registry_run(&["publish", &logical_id, "1.0.0", "--actor", "alice"]).status.success());

    let resolve = Command::new(capsule_bin())
        .args(["resolve", "--db", db.to_str().unwrap(), "privacy review", "--domain", "policy", "--about", "retained", "--plan"])
        .output()
        .unwrap();
    assert!(resolve.status.success(), "{}", String::from_utf8_lossy(&resolve.stderr));
    let stdout = String::from_utf8_lossy(&resolve.stdout);
    assert!(stdout.contains("resolved 1 capsule(s)"));
    assert!(stdout.contains(&logical_id));
}

#[test]
fn pack_sign_and_unpack_verify_round_trip() {
    let dir = tempdir();
    let keygen = Command::new(capsule_bin()).args(["keygen", "--out-prefix"]).arg(dir.join("signer")).output().unwrap();
    assert!(keygen.status.success());
    let other_keygen = Command::new(capsule_bin()).args(["keygen", "--out-prefix"]).arg(dir.join("other")).output().unwrap();
    assert!(other_keygen.status.success());

    let capsule = repo_root().join("examples/capsules/data-retention-90-days.json");
    let package = dir.join("signed.kcap");
    let pack = Command::new(capsule_bin())
        .args(["pack", capsule.to_str().unwrap(), "-o"])
        .arg(&package)
        .args(["--sign"])
        .arg(dir.join("signer.key"))
        .output()
        .unwrap();
    assert!(pack.status.success(), "{}", String::from_utf8_lossy(&pack.stderr));
    assert!(String::from_utf8_lossy(&pack.stdout).contains("signed by"));

    // verifies against the correct key
    let out_ok = dir.join("out-ok");
    let unpack_ok = Command::new(capsule_bin())
        .args(["unpack"])
        .arg(&package)
        .args(["-o"])
        .arg(&out_ok)
        .args(["--verify-key"])
        .arg(dir.join("signer.pub"))
        .output()
        .unwrap();
    assert!(unpack_ok.status.success(), "{}", String::from_utf8_lossy(&unpack_ok.stderr));
    assert!(String::from_utf8_lossy(&unpack_ok.stdout).contains("signature verified"));

    // fails against the wrong key
    let out_bad = dir.join("out-bad");
    let unpack_bad = Command::new(capsule_bin())
        .args(["unpack"])
        .arg(&package)
        .args(["-o"])
        .arg(&out_bad)
        .args(["--verify-key"])
        .arg(dir.join("other.pub"))
        .output()
        .unwrap();
    assert!(!unpack_bad.status.success());
}

#[test]
fn export_then_import_replays_lifecycle_state() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let src_db = dir.join("src.sqlite3");
    let src_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&src_db).args(args).output().unwrap()
    };
    assert!(src_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());
    assert!(src_run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]).status.success());
    assert!(src_run(&["transition", &logical_id, "1.0.0", "approved", "--actor", "alice"]).status.success());
    assert!(src_run(&["publish", &logical_id, "1.0.0", "--actor", "alice"]).status.success());

    let export_path = dir.join("export.jsonl");
    let export = src_run(&["export", "--out", export_path.to_str().unwrap()]);
    assert!(export.status.success(), "{}", String::from_utf8_lossy(&export.stderr));
    assert!(String::from_utf8_lossy(&export.stdout).contains("exported 1"));

    let dst_db = dir.join("dst.sqlite3");
    let import = Command::new(capsule_bin())
        .args(["registry", "--db"])
        .arg(&dst_db)
        .args(["import"])
        .arg(&export_path)
        .args(["--actor", "bulk-import"])
        .output()
        .unwrap();
    assert!(import.status.success(), "{}", String::from_utf8_lossy(&import.stderr));
    assert!(String::from_utf8_lossy(&import.stdout).contains("imported 1"));

    let dst_get = Command::new(capsule_bin()).args(["registry", "--db"]).arg(&dst_db).args(["get", &logical_id, "1.0.0"]).output().unwrap();
    assert!(dst_get.status.success());
    assert!(String::from_utf8_lossy(&dst_get.stdout).contains("Published"), "imported capsule should have replayed to Published");
}

#[test]
fn pack_create_and_resolve_round_trip() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let db = dir.join("registry.sqlite3");
    let registry_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&db).args(args).output().unwrap()
    };
    assert!(registry_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "approved", "--actor", "alice"]).status.success());
    assert!(registry_run(&["publish", &logical_id, "1.0.0", "--actor", "alice"]).status.success());

    let pack_path = dir.join("demo.pack.json");
    let create = Command::new(capsule_bin())
        .args(["pack-create"])
        .arg(&pack_path)
        .args(["--name", "demo-pack", "--description", "a test pack"])
        .arg(&logical_id)
        .output()
        .unwrap();
    assert!(create.status.success(), "{}", String::from_utf8_lossy(&create.stderr));
    assert!(pack_path.exists());

    let resolve = Command::new(capsule_bin()).args(["pack-resolve"]).arg(&pack_path).args(["--db"]).arg(&db).output().unwrap();
    assert!(resolve.status.success(), "{}", String::from_utf8_lossy(&resolve.stderr));
    assert!(String::from_utf8_lossy(&resolve.stdout).contains("all members resolved and published"));
}

#[test]
fn pack_resolve_fails_when_a_member_is_unpublished() {
    let dir = tempdir();
    let pack_path = dir.join("empty.pack.json");
    fs::write(&pack_path, r#"{"name": "x", "description": "", "members": ["kc://a/b/missing"]}"#).unwrap();
    let db = dir.join("registry.sqlite3");

    let resolve = Command::new(capsule_bin()).args(["pack-resolve"]).arg(&pack_path).args(["--db"]).arg(&db).output().unwrap();
    assert!(!resolve.status.success());
    assert!(String::from_utf8_lossy(&resolve.stdout).contains("not found"));
}

#[test]
fn bench_remote_resolve_prints_latency_stats_against_a_live_server() {
    let registry = capsule_registry::Registry::open_in_memory().unwrap();
    let (addr, _handle) = capsule_api::Server::new(registry).spawn_background().unwrap();

    let out = Command::new(capsule_bin())
        .args(["bench", "remote-resolve"])
        .arg(addr.to_string())
        .args(["privacy review", "--iterations", "5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("n=5"), "expected iteration count in output, got: {stdout}");
    assert!(stdout.contains("POST /resolve"));
}

#[test]
fn bench_compile_prints_latency_stats_for_a_real_source() {
    let out = Command::new(capsule_bin())
        .args(["bench", "compile"])
        .arg(repo_root().join("docs/specification/knowledge-capsule-spec.md"))
        .args(["--domain", "policy", "--topic", "retention", "--owner", "alice", "--iterations", "5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("n=5"), "expected iteration count in output, got: {stdout}");
    assert!(stdout.contains("median="));
}

#[test]
fn bench_resolve_prints_latency_stats_against_a_populated_registry() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let db = dir.join("registry.sqlite3");
    let registry_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&db).args(args).output().unwrap()
    };
    assert!(registry_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "approved", "--actor", "alice"]).status.success());
    assert!(registry_run(&["publish", &logical_id, "1.0.0", "--actor", "alice"]).status.success());

    let out = Command::new(capsule_bin())
        .args(["bench", "resolve", "privacy review", "--db"])
        .arg(&db)
        .args(["--domain", "policy", "--iterations", "10"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("n=10"), "expected iteration count in output, got: {stdout}");
}

#[test]
fn registry_register_with_policy_refuses_a_violating_capsule_but_allows_a_clean_one() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();

    let policy_path = dir.join("policy.json");
    let declared_domain = draft["governance"]["domain"].as_str().unwrap().to_string();
    fs::write(&policy_path, format!(r#"{{"rules": [{{"type": "allowed_domains", "domains": ["not-{declared_domain}"]}}]}}"#)).unwrap();

    let db = dir.join("registry.sqlite3");
    let reject = Command::new(capsule_bin())
        .arg("registry")
        .arg("--db")
        .arg(&db)
        .args(["register"])
        .arg(&draft_path)
        .args(["--actor", "alice", "--policy"])
        .arg(&policy_path)
        .output()
        .unwrap();
    assert!(!reject.status.success());
    assert!(String::from_utf8_lossy(&reject.stderr).contains("allowed_domains"));

    let allow_policy_path = dir.join("allow.json");
    fs::write(&allow_policy_path, format!(r#"{{"rules": [{{"type": "allowed_domains", "domains": ["{declared_domain}"]}}]}}"#)).unwrap();
    let accept = Command::new(capsule_bin())
        .arg("registry")
        .arg("--db")
        .arg(&db)
        .args(["register"])
        .arg(&draft_path)
        .args(["--actor", "alice", "--policy"])
        .arg(&allow_policy_path)
        .output()
        .unwrap();
    assert!(accept.status.success(), "{}", String::from_utf8_lossy(&accept.stderr));
}

#[test]
fn registry_backup_and_restore_round_trip_with_encryption() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let db = dir.join("registry.sqlite3");
    let registry_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&db).args(args).output().unwrap()
    };
    assert!(registry_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());

    let key_path = dir.join("data.key");
    let datakey = Command::new(capsule_bin()).args(["datakey", "--out"]).arg(&key_path).output().unwrap();
    assert!(datakey.status.success(), "{}", String::from_utf8_lossy(&datakey.stderr));
    assert!(key_path.exists());

    let backup_path = dir.join("backup.enc");
    let backup = registry_run(&["backup", "--out", backup_path.to_str().unwrap(), "--encrypt-key-file", key_path.to_str().unwrap()]);
    assert!(backup.status.success(), "{}", String::from_utf8_lossy(&backup.stderr));
    assert_ne!(fs::read(&backup_path).unwrap(), fs::read(&db).unwrap(), "backup must actually be encrypted, not a plain copy");

    let restored_path = dir.join("restored.sqlite3");
    let restore = Command::new(capsule_bin())
        .args(["registry", "--db"])
        .arg(&db) // --db is required by the global arg but unused by `restore`
        .args(["restore"])
        .arg(&backup_path)
        .args(["--out"])
        .arg(&restored_path)
        .args(["--decrypt-key-file"])
        .arg(&key_path)
        .output()
        .unwrap();
    assert!(restore.status.success(), "{}", String::from_utf8_lossy(&restore.stderr));

    let get = Command::new(capsule_bin()).args(["registry", "--db"]).arg(&restored_path).args(["get", &logical_id, "1.0.0"]).output().unwrap();
    assert!(get.status.success());
    assert!(String::from_utf8_lossy(&get.stdout).contains(&logical_id));

    // Restoring with the wrong key must fail loudly, not silently produce garbage.
    let wrong_key_path = dir.join("wrong.key");
    let wrong_key_gen = Command::new(capsule_bin()).args(["datakey", "--out"]).arg(&wrong_key_path).output().unwrap();
    assert!(wrong_key_gen.status.success());
    let bad_restore_path = dir.join("bad-restore.sqlite3");
    let bad_restore = Command::new(capsule_bin())
        .args(["registry", "--db"])
        .arg(&db)
        .args(["restore"])
        .arg(&backup_path)
        .args(["--out"])
        .arg(&bad_restore_path)
        .args(["--decrypt-key-file"])
        .arg(&wrong_key_path)
        .output()
        .unwrap();
    assert!(!bad_restore.status.success());
}

#[test]
fn registry_audit_export_dumps_the_full_ledger() {
    let dir = tempdir();
    let mut draft: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("examples/capsules/data-retention-90-days.json")).unwrap()).unwrap();
    draft["lifecycle_state"] = serde_json::Value::String("draft".to_string());
    let draft_path = dir.join("draft.json");
    fs::write(&draft_path, serde_json::to_string_pretty(&draft).unwrap()).unwrap();
    let logical_id = draft["logical_id"].as_str().unwrap().to_string();

    let db = dir.join("registry.sqlite3");
    let registry_run = |args: &[&str]| -> std::process::Output {
        Command::new(capsule_bin()).arg("registry").arg("--db").arg(&db).args(args).output().unwrap()
    };
    assert!(registry_run(&["register", draft_path.to_str().unwrap(), "--actor", "alice"]).status.success());
    assert!(registry_run(&["transition", &logical_id, "1.0.0", "in_review", "--actor", "alice"]).status.success());

    let out_path = dir.join("audit.jsonl");
    let export = registry_run(&["audit-export", "--out", out_path.to_str().unwrap()]);
    assert!(export.status.success(), "{}", String::from_utf8_lossy(&export.stderr));
    assert!(String::from_utf8_lossy(&export.stdout).contains("exported 2"));

    let lines: Vec<String> = fs::read_to_string(&out_path).unwrap().lines().map(|l| l.to_string()).collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(first["event_type"], "register");
    let second: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(second["event_type"], "transition");
}

fn tempdir() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("capsule-cli-test-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
