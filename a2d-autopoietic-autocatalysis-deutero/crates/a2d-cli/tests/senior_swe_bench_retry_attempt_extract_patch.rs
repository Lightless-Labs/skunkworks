use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn git_hash_object_bytes(bytes: &[u8]) -> String {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn git hash-object");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(bytes)
        .expect("write stdin");
    let output = child.wait_with_output().expect("git hash-object output");
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn write_plan(
    root: &std::path::Path,
    artifact_bytes: &[u8],
    candidate_patch_path: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    fs::create_dir_all(root).unwrap();
    let artifact = root.join("candidate.artifact");
    fs::write(&artifact, artifact_bytes).unwrap();
    let patch_hash = git_hash_object_bytes(artifact_bytes);
    let artifact_hash = git_hash_object_bytes(artifact_bytes);
    let plan = root.join("retry-attempt-plan.json");
    fs::write(
        &plan,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-attempt-plan.v1",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "attempt_index": 0,
            "max_attempts": 2,
            "decision": "extract_and_evaluate_candidate_patch",
            "selected_artifact": {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": artifact_hash,
                "bytes": artifact_bytes.len()
            },
            "candidate_patch_hash": patch_hash,
            "planned_outputs": {
                "candidate_patch": candidate_patch_path,
                "local_evaluation": root.join("attempt-0/local-evaluation.json")
            },
            "evaluate_args": ["senior-swe-bench-evaluate", "--output", root.join("attempt-0/local-evaluation.json"), "--", "./evaluate.sh"],
            "retry_step_args": ["senior-swe-bench-retry-step", "--local-evaluation", root.join("attempt-0/local-evaluation.json")],
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "github_solution_search_allowed": false
        }))
        .unwrap(),
    )
    .unwrap();
    (plan, artifact)
}

#[test]
fn retry_attempt_extract_patch_materializes_patch_and_emits_next_args() {
    let root = std::env::temp_dir().join(format!("a2d-retry-attempt-extract-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let patch_path = root.join("attempt-0/candidate.patch");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (plan, _artifact) = write_plan(&root, diff, &patch_path);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            plan.to_str().unwrap(),
        ])
        .output()
        .expect("run extract command");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&patch_path).unwrap(), diff);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-extraction.v1")
    );
    assert_eq!(
        value["candidate_patch_path"].as_str(),
        Some(patch_path.to_str().unwrap())
    );
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));
    assert_eq!(
        value["evaluator_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["evaluate_args"].as_array().unwrap()[0].as_str(),
        Some("senior-swe-bench-evaluate")
    );
    assert_eq!(
        value["retry_step_args"].as_array().unwrap()[0].as_str(),
        Some("senior-swe-bench-retry-step")
    );

    let idempotent = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            plan.to_str().unwrap(),
        ])
        .output()
        .expect("rerun extract command");
    assert_eq!(idempotent.status.code(), Some(0));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn retry_attempt_extract_patch_fails_closed_for_unsafe_or_stale_plans() {
    let root =
        std::env::temp_dir().join(format!("a2d-retry-attempt-extract-bad-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let patch_path = root.join("attempt-0/candidate.patch");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (plan, artifact) = write_plan(&root, diff, &patch_path);

    let mut stop_plan: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&plan).unwrap()).unwrap();
    stop_plan["decision"] = serde_json::Value::String("stop".to_string());
    let stop_path = root.join("stop-plan.json");
    fs::write(
        &stop_path,
        serde_json::to_string_pretty(&stop_plan).unwrap(),
    )
    .unwrap();
    let stopped = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            stop_path.to_str().unwrap(),
        ])
        .output()
        .expect("run extract command");
    assert_eq!(stopped.status.code(), Some(1));
    assert!(!patch_path.exists());
    assert!(String::from_utf8_lossy(&stopped.stderr).contains("decision"));

    let mut tampered_bytes = diff.to_vec();
    tampered_bytes[0] = b'X';
    fs::write(&artifact, tampered_bytes).unwrap();
    let tampered = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            plan.to_str().unwrap(),
        ])
        .output()
        .expect("run extract command");
    assert_eq!(tampered.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&tampered.stderr).contains("hash mismatch"));
    assert!(!patch_path.exists());

    fs::write(&artifact, diff).unwrap();
    fs::create_dir_all(patch_path.parent().unwrap()).unwrap();
    fs::write(&patch_path, b"stale patch").unwrap();
    let stale = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            plan.to_str().unwrap(),
        ])
        .output()
        .expect("run extract command");
    assert_eq!(stale.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&stale.stderr).contains("already exists with different bytes"));

    let public = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\nhttps://github.com/owner/repo/pull/1\n";
    let public_patch = root.join("attempt-public/candidate.patch");
    let (public_plan, _public_artifact) = write_plan(&root.join("public"), public, &public_patch);
    let public_rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-extract-patch",
            public_plan.to_str().unwrap(),
        ])
        .output()
        .expect("run extract command");
    assert_eq!(public_rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&public_rejected.stderr).contains("public GitHub"));

    let _ = fs::remove_dir_all(root);
}
