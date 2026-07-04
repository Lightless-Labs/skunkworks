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
    step_execution: std::path::PathBuf,
    patch: std::path::PathBuf,
    fitness_evidence: std::path::PathBuf,
}

fn write_fixture(name: &str, all_tests_pass: bool) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "a2d-retry-attempt-step-evidence-{name}-{}",
        unique_suffix()
    ));
    let attempt = root.join("attempt-0");
    let checkout = root.join("checkout");
    let src = checkout.join("src");
    fs::create_dir_all(&attempt).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "old\n").unwrap();
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&checkout)
        .status()
        .expect("git init checkout");

    let cycle_input = root.join("cycle-input.json");
    let retry_plan = root.join("retry-plan.json");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(&retry_plan, sample_retry_plan()).unwrap();

    let artifact = root.join("candidate.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&artifact, diff).unwrap();
    let patch = attempt.join("candidate.patch");
    fs::write(&patch, diff).unwrap();
    let fitness_evidence = attempt.join("fitness-evidence.json");
    write_fitness_evidence(&fitness_evidence, all_tests_pass, &patch, &artifact);
    let local_evaluation = attempt.join("local-evaluation.json");
    fs::write(
        &local_evaluation,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-local-evaluation.v1",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "evaluator": "provided_local_command",
            "status": "passed",
            "exit_code": 0,
            "github_solution_search_allowed": false,
            "candidate_patch": patch,
            "candidate_patch_hash": git_hash_object_bytes(diff),
            "candidate_patch_artifact_path": artifact,
            "candidate_patch_artifact_hash": git_hash_object_bytes(diff),
            "candidate_patch_applied": true,
            "evaluator_checkout_mode": "isolated_copy",
            "original_checkout_mutated": false,
            "candidate_patch_preflight_checked": true,
            "candidate_patch_preflight_status": "passed",
            "candidate_patch_preflight_command": "git apply --check --whitespace=nowarn -- candidate.patch",
            "checkout": checkout,
            "evaluator_command": ["/bin/true"],
            "fitness_evidence_path": fitness_evidence,
            "source_revision": current_crates_revision(),
            "source_tree_dirty": current_crates_dirty(),
            "source_diff_scope": "crates",
            "source_diff_hash": current_crates_diff_hash(),
            "evidence_command": "test fixture"
        }))
        .unwrap(),
    )
    .unwrap();

    let step_execution = root.join("step-execution.json");
    fs::write(
        &step_execution,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-attempt-step-execution.v1",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "attempt_index": 0,
            "candidate_patch_path": patch,
            "candidate_patch_hash": git_hash_object_bytes(diff),
            "selected_artifact": {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": git_hash_object_bytes(diff),
                "bytes": diff.len()
            },
            "evaluate_args": [
                "senior-swe-bench-evaluate",
                "--task-cycle-input", cycle_input,
                "--candidate-patch-artifact", artifact,
                "--extracted-candidate-patch", patch,
                "--checkout", checkout,
                "--apply-candidate-patch",
                "--output", local_evaluation,
                "--", "/bin/true"
            ],
            "retry_step_args": [
                "senior-swe-bench-retry-step",
                "--retry-plan", retry_plan,
                "--attempt-index", "0",
                "--task-cycle-input", cycle_input,
                "--local-evaluation", local_evaluation
            ],
            "evaluate_exit_code": 0,
            "local_evaluation_path": local_evaluation,
            "local_evaluation_status": "passed",
            "retry_step": {
                "schema_version": "a2d.senior-swe-bench-cycle-retry-step.v1",
                "task_id": "task-hard",
                "repo": "owner/repo",
                "attempt_index": 0,
                "evaluation_status": "passed",
                "decision": "inspect_fitness_evidence",
                "fitness_evidence_path": fitness_evidence,
                "fitness_evidence_inspect_args": [
                    "fitness-evidence-inspect",
                    fitness_evidence,
                    "--require-all-tests-pass"
                ],
                "provider_invocations_started": false,
                "evaluator_invocations_started": false,
                "fitness_claim_allowed_before_evidence": false,
                "github_solution_search_allowed": false
            },
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "prior_evaluator_invocations_started": true,
            "retry_step_started": true,
            "fitness_evidence_inspection_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "github_solution_search_allowed": false
        }))
        .unwrap(),
    )
    .unwrap();

    Fixture {
        root,
        step_execution,
        patch,
        fitness_evidence,
    }
}

fn sample_cycle_input() -> &'static str {
    r#"{
  "requirements": "Do not search GitHub. Return a unified diff candidate patch.",
  "design": "Use local checkout context only.",
  "plan": "Return only a diff.",
  "benchmark_context": {
    "schema_version": "a2d.senior-swe-bench-task-package.v1",
    "task_id": "task-hard",
    "repo": "owner/repo",
    "github_solution_search_allowed": false
  },
  "evaluation": {
    "status": "not_evaluated",
    "evaluator": "official_senior_swe_bench",
    "fitness": null
  }
}
"#
}

fn sample_retry_plan() -> &'static str {
    r#"{
  "schema_version": "a2d.senior-swe-bench-cycle-retry-plan.v1",
  "task_id": "task-hard",
  "repo": "owner/repo",
  "github_solution_search_allowed": false,
  "max_attempts": 1,
  "provider_invocations_started": false,
  "fitness_claim_allowed_before_evidence": false,
  "success_requires": ["a2d.fitness-evidence.v1", "actual_tests_evaluated:true", "non_regressing:true", "all_tests_pass:true"],
  "stop_criteria": ["candidate_patch_extraction_failed", "evaluation_passed_with_valid_fitness_evidence", "evaluation_rejected_for_policy_or_binding_mismatch", "max_attempts_exhausted"],
  "information_barriers": {
    "public_github_solution_search_allowed": false,
    "official_hidden_holdout_output_to_coder": "redacted",
    "local_evaluator_output_to_coder": "only_when_feedback_visibility_is_public_local_test_output",
    "runtime_artifacts_seeded_from_cycle_input": false
  },
  "attempts": [{
    "attempt_index": 0,
    "cycle_input_source": "initial_task_cycle_input",
    "required_gates": ["run_cycle_input_with_output_artifacts", "extract_unified_diff_candidate_patch", "evaluate_candidate_patch_against_checkout", "inspect_a2d_fitness_evidence_when_evaluator_passes"],
    "on_patch_extraction_failure": "stop_fail_closed_without_evaluator_or_fitness_claim",
    "on_evaluation_passed": "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
    "on_evaluation_failed": "stop_attempts_exhausted_without_fitness_claim"
  }],
  "note": "planning artifact only"
}
"#
}

fn write_fitness_evidence(
    path: &std::path::Path,
    all_tests_pass: bool,
    patch: &std::path::Path,
    artifact: &std::path::Path,
) {
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
            "candidate_patch_hash": git_hash_object_bytes(b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"),
            "candidate_patch_path": patch,
            "candidate_patch_artifact_path": artifact,
            "candidate_patch_artifact_hash": git_hash_object_bytes(b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"),
            "evaluator_kind": "provided_local_command",
            "evidence_command": "fixture-only fitness evidence for retry-attempt step-evidence CLI test"
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn retry_attempt_step_evidence_runs_planned_inspection_once() {
    let fixture = write_fixture("passed", true);
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1")
    );
    assert_eq!(
        value["fitness_evidence_inspection_started"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["fitness_evidence_inspection_passed"].as_bool(),
        Some(true)
    );
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));
    assert_eq!(
        value["evaluator_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(value["prior_retry_step_started"].as_bool(), Some(true));
    assert_eq!(
        value["fitness_claim_allowed_after_evidence_inspection"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["fitness_evidence_summary"]["source_diff_hash"].as_str(),
        Some(current_crates_diff_hash().as_str())
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_non_inspect_decision() {
    let fixture = write_fixture("stop", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_execution).unwrap()).unwrap();
    value["retry_step"]["decision"] = serde_json::Value::String("stop".to_string());
    value["retry_step"]["stop_reason"] = serde_json::Value::String("done".to_string());
    fs::write(
        &fixture.step_execution,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("inspect_fitness_evidence"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_already_inspected_input() {
    let fixture = write_fixture("already-inspected", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_execution).unwrap()).unwrap();
    value["fitness_evidence_inspection_started"] = serde_json::Value::Bool(true);
    fs::write(
        &fixture.step_execution,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not have inspected"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_incomplete_step_execution_provenance() {
    let fixture = write_fixture("incomplete-provenance", true);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_execution).unwrap()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("local_evaluation_path");
    fs::write(
        &fixture.step_execution,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("local_evaluation_path"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_failed_local_evaluation_with_inspect_decision() {
    let fixture = write_fixture("failed-local-forged-inspect", true);
    let mut step: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.step_execution).unwrap()).unwrap();
    let local_path = std::path::PathBuf::from(step["local_evaluation_path"].as_str().unwrap());
    let mut local: serde_json::Value =
        serde_json::from_slice(&fs::read(&local_path).unwrap()).unwrap();
    local["status"] = serde_json::Value::String("failed".to_string());
    local["exit_code"] = serde_json::Value::Number(2.into());
    fs::write(&local_path, serde_json::to_vec_pretty(&local).unwrap()).unwrap();
    step["evaluate_exit_code"] = serde_json::Value::Number(2.into());
    step["local_evaluation_status"] = serde_json::Value::String("failed".to_string());
    step["retry_step"]["evaluation_status"] = serde_json::Value::String("failed".to_string());
    fs::write(
        &fixture.step_execution,
        serde_json::to_vec_pretty(&step).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires a passed local evaluation"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_failed_fitness_evidence() {
    let fixture = write_fixture("failed-evidence", false);
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("fitness-evidence-inspect failed"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_tampered_patch_before_inspection() {
    let fixture = write_fixture("tampered", true);
    fs::write(&fixture.patch, "tampered\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("candidate patch hash mismatch"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_evidence_rejects_stale_source_hash() {
    let fixture = write_fixture("stale-source", true);
    let mut evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.fitness_evidence).unwrap()).unwrap();
    evidence["source_diff_hash"] =
        serde_json::Value::String("0123456789abcdef0123456789abcdef01234567".to_string());
    fs::write(
        &fixture.fitness_evidence,
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step-evidence",
            fixture.step_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step evidence");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("source_diff_hash"));

    let _ = fs::remove_dir_all(fixture.root);
}
