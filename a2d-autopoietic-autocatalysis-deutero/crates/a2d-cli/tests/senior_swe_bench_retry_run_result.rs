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

fn git_output(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn repo_relative_crates_scope() -> String {
    let prefix = git_output(&["rev-parse", "--show-prefix"]);
    let project_prefix = prefix
        .find("/crates/")
        .map(|index| &prefix[..index + 1])
        .unwrap_or(prefix.as_str());
    format!("{project_prefix}crates")
}

fn current_crates_revision() -> String {
    git_output(&[
        "rev-parse",
        "--short",
        &format!("HEAD:{}", repo_relative_crates_scope()),
    ])
}

fn current_crates_dirty() -> bool {
    !git_output(&[
        "status",
        "--short",
        "--",
        &format!(":(top){}", repo_relative_crates_scope()),
    ])
    .is_empty()
}

fn current_crates_diff_hash() -> String {
    let diff = Command::new("git")
        .args([
            "diff",
            "--binary",
            "HEAD",
            "--",
            &format!(":(top){}", repo_relative_crates_scope()),
        ])
        .output()
        .expect("run git diff");
    assert!(diff.status.success());
    git_hash_object_bytes(&diff.stdout)
}

struct Fixture {
    root: std::path::PathBuf,
    step_evidence: std::path::PathBuf,
}

fn write_fixture(name: &str, all_tests_pass: bool) -> Fixture {
    let root =
        std::env::temp_dir().join(format!("a2d-retry-run-result-{name}-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let patch = root.join("candidate.patch");
    let artifact = root.join("candidate.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&patch, diff).unwrap();
    fs::write(&artifact, diff).unwrap();
    let fitness_evidence = root.join("fitness-evidence.json");
    write_fitness_evidence(&fitness_evidence, all_tests_pass, &patch, &artifact);
    let step_evidence = root.join("step-evidence.json");
    fs::write(
        &step_evidence,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "attempt_index": 0,
            "fitness_evidence_path": fitness_evidence,
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "prior_evaluator_invocations_started": true,
            "prior_retry_step_started": true,
            "fitness_evidence_inspection_started": true,
            "fitness_evidence_inspection_passed": true,
            "fitness_claim_allowed_before_evidence": false,
            "fitness_claim_allowed_after_evidence_inspection": true,
            "github_solution_search_allowed": false,
            "fitness_evidence_summary": {
                "schema_version": "a2d.fitness-evidence.v1",
                "actual_tests_evaluated": true,
                "non_regressing": true,
                "fitness": if all_tests_pass { 1.0 } else { 0.67 },
                "passed": if all_tests_pass { 3 } else { 2 },
                "failed": if all_tests_pass { 0 } else { 1 },
                "total": 3,
                "source_revision": current_crates_revision(),
                "source_tree_dirty": current_crates_dirty(),
                "source_diff_hash": current_crates_diff_hash(),
                "candidate_patch_hash": git_hash_object_bytes(diff),
                "candidate_patch_path": patch,
                "candidate_patch_artifact_path": artifact,
                "candidate_patch_artifact_hash": git_hash_object_bytes(diff),
                "evaluator_kind": "provided_local_command"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    Fixture {
        root,
        step_evidence,
    }
}

fn write_fitness_evidence(
    path: &std::path::Path,
    all_tests_pass: bool,
    patch: &std::path::Path,
    artifact: &std::path::Path,
) {
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let failed = if all_tests_pass { 0 } else { 1 };
    let passed = if all_tests_pass { 3 } else { 2 };
    let fitness = if all_tests_pass { 1.0 } else { 0.67 };
    fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.fitness-evidence.v1",
            "cycle": 0,
            "actual_tests_evaluated": true,
            "fitness": fitness,
            "delta_from_last_non_regressing_fitness": 0.0,
            "non_regressing": true,
            "diagnostic_present": true,
            "passed": passed,
            "failed": failed,
            "total": 3,
            "results": [
                {"name": "compiles", "passed": true},
                {"name": "all_tests_pass", "passed": all_tests_pass},
                {"name": "hidden_acceptance", "passed": all_tests_pass}
            ],
            "failed_cases": if all_tests_pass { Vec::<&str>::new() } else { vec!["hidden_acceptance", "all_tests_pass"] },
            "source_revision": current_crates_revision(),
            "source_tree_dirty": current_crates_dirty(),
            "source_diff_scope": "crates",
            "source_diff_hash": current_crates_diff_hash(),
            "candidate_patch_hash": git_hash_object_bytes(diff),
            "candidate_patch_path": patch,
            "candidate_patch_artifact_path": artifact,
            "candidate_patch_artifact_hash": git_hash_object_bytes(diff),
            "evaluator_kind": "provided_local_command",
            "evidence_command": "fixture-only fitness evidence for retry-run-result CLI test"
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn retry_run_result_summarizes_inspected_local_wrapper_evidence_without_official_overclaim() {
    let fixture = write_fixture("passed", true);
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-run-result",
            fixture.step_evidence.to_str().unwrap(),
        ])
        .output()
        .expect("run retry run result");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-run-result.v1")
    );
    assert_eq!(value["status"].as_str(), Some("success"));
    assert_eq!(
        value["final_evaluator_kind"].as_str(),
        Some("provided_local_command")
    );
    assert_eq!(
        value["official_senior_swe_bench_mastery"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_claim_allowed_after_evidence_inspection"].as_bool(),
        Some(true)
    );
    assert!(
        value["fitness_claim_boundary"]
            .as_str()
            .unwrap()
            .contains("do not claim official Senior SWE-Bench mastery")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_run_result_rejects_uninspected_step_evidence() {
    let fixture = write_fixture("uninspected", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_evidence).unwrap()).unwrap();
    value["fitness_evidence_inspection_started"] = serde_json::Value::Bool(false);
    value["fitness_evidence_inspection_passed"] = serde_json::Value::Bool(false);
    fs::write(
        &fixture.step_evidence,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-run-result",
            fixture.step_evidence.to_str().unwrap(),
        ])
        .output()
        .expect("run retry run result");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("passed fitness evidence inspection"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_run_result_rejects_failed_underlying_fitness_evidence() {
    let fixture = write_fixture("failed", false);
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-run-result",
            fixture.step_evidence.to_str().unwrap(),
        ])
        .output()
        .expect("run retry run result");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("all_tests_pass"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_run_result_rejects_summary_evaluator_kind_overclaim() {
    let fixture = write_fixture("overclaim", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_evidence).unwrap()).unwrap();
    value["fitness_evidence_summary"]["evaluator_kind"] =
        serde_json::Value::String("official_senior_swe_bench".to_string());
    fs::write(
        &fixture.step_evidence,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-run-result",
            fixture.step_evidence.to_str().unwrap(),
        ])
        .output()
        .expect("run retry run result");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("fitness_evidence_summary does not match")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_run_result_rejects_forged_summary_counts() {
    let fixture = write_fixture("forged-summary", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_evidence).unwrap()).unwrap();
    value["fitness_evidence_summary"]["passed"] = serde_json::Value::from(999);
    fs::write(
        &fixture.step_evidence,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-run-result",
            fixture.step_evidence.to_str().unwrap(),
        ])
        .output()
        .expect("run retry run result");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("fitness_evidence_summary does not match")
    );

    let _ = fs::remove_dir_all(fixture.root);
}
