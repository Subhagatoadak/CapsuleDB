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

fn tempdir() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("capsule-cli-test-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
