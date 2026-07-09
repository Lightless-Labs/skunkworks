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
  "max_attempts": 2,
  "provider_invocations_started": false,
  "fitness_claim_allowed_before_evidence": false,
  "success_requires": [
    "a2d.fitness-evidence.v1",
    "actual_tests_evaluated:true",
    "non_regressing:true",
    "all_tests_pass:true"
  ],
  "stop_criteria": [
    "candidate_patch_extraction_failed",
    "evaluation_passed_with_valid_fitness_evidence",
    "evaluation_rejected_for_policy_or_binding_mismatch",
    "max_attempts_exhausted"
  ],
  "information_barriers": {
    "public_github_solution_search_allowed": false,
    "official_hidden_holdout_output_to_coder": "redacted",
    "local_evaluator_output_to_coder": "only_when_feedback_visibility_is_public_local_test_output",
    "runtime_artifacts_seeded_from_cycle_input": false
  },
  "attempts": [
    {
      "attempt_index": 0,
      "cycle_input_source": "initial_task_cycle_input",
      "required_gates": ["run_cycle_input_with_output_artifacts", "extract_unified_diff_candidate_patch", "evaluate_candidate_patch_against_checkout", "inspect_a2d_fitness_evidence_when_evaluator_passes"],
      "on_patch_extraction_failure": "stop_fail_closed_without_evaluator_or_fitness_claim",
      "on_evaluation_passed": "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
      "on_evaluation_failed": "build_next_cycle_input_with_senior_swe_bench_cycle_input_feedback"
    },
    {
      "attempt_index": 1,
      "cycle_input_source": "feedback_from_previous_local_evaluation",
      "required_gates": ["run_cycle_input_with_output_artifacts", "extract_unified_diff_candidate_patch", "evaluate_candidate_patch_against_checkout", "inspect_a2d_fitness_evidence_when_evaluator_passes"],
      "on_patch_extraction_failure": "stop_fail_closed_without_evaluator_or_fitness_claim",
      "on_evaluation_passed": "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
      "on_evaluation_failed": "stop_attempts_exhausted_without_fitness_claim"
    }
  ],
  "note": "planning artifact only"
}
"#
}

struct Fixture {
    root: std::path::PathBuf,
    evaluation: std::path::PathBuf,
    patch: std::path::PathBuf,
}

fn write_fixture(name: &str, status: &str) -> Fixture {
    let root =
        std::env::temp_dir().join(format!("a2d-retry-attempt-step-{name}-{}", unique_suffix()));
    let checkout = root.join("checkout");
    let src = checkout.join("src");
    let attempt = root.join("attempt-0");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&attempt).unwrap();
    fs::write(src.join("lib.rs"), "old\n").unwrap();
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&checkout)
        .status()
        .expect("git init");

    let cycle_input = root.join("cycle-input.json");
    let retry_plan = root.join("retry-plan.json");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(&retry_plan, sample_retry_plan()).unwrap();
    let artifact = root.join("candidate.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&artifact, diff).unwrap();
    let patch = attempt.join("candidate.patch");
    fs::write(&patch, diff).unwrap();
    let local_evaluation = attempt.join("local-evaluation.json");
    let fitness_evidence = attempt.join("fitness-evidence.json");
    let exit_code = if status == "passed" { 0 } else { 2 };
    let mut local = serde_json::json!({
        "schema_version": "a2d.senior-swe-bench-local-evaluation.v1",
        "task_id": "task-hard",
        "repo": "owner/repo",
        "evaluator": "provided_local_command",
        "status": status,
        "exit_code": exit_code,
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
        "candidate_patch_preflight_command": "git apply --check --recount --whitespace=nowarn -- candidate.patch",
        "checkout": checkout,
        "evaluator_command": ["/bin/true"],
        "source_revision": current_crates_revision(),
        "source_tree_dirty": current_crates_dirty(),
        "source_diff_scope": "crates",
        "source_diff_hash": current_crates_diff_hash(),
        "evidence_command": "test fixture"
    });
    if status == "passed" {
        local["fitness_evidence_path"] =
            serde_json::Value::String(fitness_evidence.to_string_lossy().to_string());
    } else {
        local["feedback_visibility"] =
            serde_json::Value::String("public_local_test_output".to_string());
        local["stdout_preview"] = serde_json::Value::String(
            "public local regression: expected /settings route to return 200".to_string(),
        );
        local["stderr_preview"] = serde_json::Value::String("missing route assertion".to_string());
    }
    fs::write(
        &local_evaluation,
        serde_json::to_vec_pretty(&local).unwrap(),
    )
    .unwrap();

    let evaluation = root.join("evaluation.json");
    fs::write(
        &evaluation,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-attempt-evaluation.v1",
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
            "evaluate_exit_code": exit_code,
            "local_evaluation_path": local_evaluation,
            "local_evaluation_status": status,
            "provider_invocations_started": false,
            "evaluator_invocations_started": true,
            "retry_step_started": false,
            "fitness_evidence_inspection_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "github_solution_search_allowed": false
        }))
        .unwrap(),
    )
    .unwrap();

    Fixture {
        root,
        evaluation,
        patch,
    }
}

#[test]
fn retry_attempt_step_runs_planned_retry_step_once_without_inspecting_fitness() {
    let fixture = write_fixture("passed", "passed");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-step-execution.v1")
    );
    assert_eq!(value["retry_step_started"].as_bool(), Some(true));
    assert_eq!(
        value["evaluator_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["prior_evaluator_invocations_started"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["fitness_evidence_inspection_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["retry_step"]["decision"].as_str(),
        Some("inspect_fitness_evidence")
    );
    assert_eq!(
        value["retry_step"]["fitness_evidence_inspect_args"]
            .as_array()
            .unwrap()[0]
            .as_str(),
        Some("fitness-evidence-inspect")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_builds_next_cycle_input_for_failed_attempt() {
    let fixture = write_fixture("failed", "failed");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["retry_step"]["decision"].as_str(),
        Some("build_next_cycle_input")
    );
    let next_cycle_input = &value["retry_step"]["next_cycle_input"];
    let next_cycle_object = next_cycle_input
        .as_object()
        .expect("next_cycle_input must remain a structured JSON object, not a string blob");
    for field in [
        "requirements",
        "design",
        "plan",
        "benchmark_context",
        "evaluation",
    ] {
        assert!(
            next_cycle_object.contains_key(field),
            "next_cycle_input missing structured field {field}: {next_cycle_input}"
        );
    }
    assert_eq!(
        next_cycle_input["requirements"].as_str(),
        Some("Do not search GitHub. Return a unified diff candidate patch.")
    );
    assert_eq!(
        next_cycle_input["benchmark_context"]["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-task-package.v1")
    );
    assert_eq!(
        next_cycle_input["benchmark_context"]["task_id"].as_str(),
        Some("task-hard")
    );
    assert_eq!(
        next_cycle_input["benchmark_context"]["repo"].as_str(),
        Some("owner/repo")
    );
    assert!(
        next_cycle_input["plan"].as_str().is_some_and(
            |plan| plan.contains("Previous plan:") && plan.contains("Return only a diff.")
        ),
        "next_cycle_input plan should remain structured retry context: {next_cycle_input}"
    );
    assert_eq!(
        next_cycle_input["evaluation"]["evaluator"].as_str(),
        Some("official_senior_swe_bench")
    );
    assert_eq!(
        next_cycle_input["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );
    assert!(next_cycle_input["evaluation"]["fitness"].is_null());
    assert_eq!(
        next_cycle_input["benchmark_context"]["github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert!(next_cycle_input.get("fitness_report").is_none());
    assert!(next_cycle_input.get("failure_report").is_none());
    let feedback_design = next_cycle_input["design"].as_str().unwrap();
    assert!(feedback_design.contains("SENIOR SWE-BENCH EVALUATOR FEEDBACK"));
    assert!(feedback_design.contains("public local regression: expected /settings route"));
    assert!(feedback_design.contains("missing route assertion"));
    assert!(feedback_design.contains("not a seeded fitness_report or failure_report"));
    assert!(feedback_design.contains("no-GitHub/public-solution-search"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_rejects_public_solution_reference_in_visible_feedback() {
    let fixture = write_fixture("solution-reference", "failed");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.evaluation).unwrap()).unwrap();
    let local_path = std::path::PathBuf::from(
        value["local_evaluation_path"]
            .as_str()
            .expect("local evaluation path"),
    );
    let mut local: serde_json::Value =
        serde_json::from_slice(&fs::read(&local_path).unwrap()).unwrap();
    local["stderr_preview"] = serde_json::Value::String(
        "see https://github.com/owner/repo/pull/123 for the fix".to_string(),
    );
    fs::write(&local_path, serde_json::to_vec_pretty(&local).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("public solution reference"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_rejects_tampered_patch_before_retry_step() {
    let fixture = write_fixture("tampered", "passed");
    fs::write(&fixture.patch, "tampered\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("candidate patch hash mismatch"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_rejects_stale_local_evaluation_source_provenance() {
    let fixture = write_fixture("stale-source", "passed");
    let mut local_path: Option<std::path::PathBuf> = None;
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.evaluation).unwrap()).unwrap();
    if let Some(path) = value["local_evaluation_path"].as_str() {
        local_path = Some(std::path::PathBuf::from(path));
    }
    let local_path = local_path.expect("local evaluation path");
    let mut local: serde_json::Value =
        serde_json::from_slice(&fs::read(&local_path).unwrap()).unwrap();
    local["source_diff_hash"] =
        serde_json::Value::String("0123456789abcdef0123456789abcdef01234567".to_string());
    fs::write(&local_path, serde_json::to_vec_pretty(&local).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("source_diff_hash"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_step_rejects_mismatched_retry_step_args_before_execution() {
    let fixture = write_fixture("mismatched-args", "passed");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.evaluation).unwrap()).unwrap();
    value["retry_step_args"][4] = serde_json::Value::String("1".to_string());
    fs::write(
        &fixture.evaluation,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-step",
            fixture.evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt step");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("attempt index does not match"));

    let _ = fs::remove_dir_all(fixture.root);
}
