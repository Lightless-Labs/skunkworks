use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
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

#[test]
fn senior_swe_bench_retry_step_builds_next_cycle_input_for_failed_nonfinal_attempt() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-retry-step-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let retry_plan = root.join("retry-plan.json");
    let cycle_input = root.join("cycle-input.json");
    let evaluation = root.join("local-evaluation.json");
    fs::write(&retry_plan, sample_retry_plan()).unwrap();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(
        &evaluation,
        r#"{
  "schema_version": "a2d.senior-swe-bench-local-evaluation.v1",
  "task_id": "task-hard",
  "repo": "owner/repo",
  "evaluator": "provided_local_command",
  "status": "failed",
  "exit_code": 2,
  "candidate_patch_hash": "abc12300",
  "github_solution_search_allowed": false,
  "feedback_visibility": "public_local_test_output",
  "stdout_preview": "unit tests failed",
  "stderr_preview": "missing route assertion"
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-step",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--local-evaluation",
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry step command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-cycle-retry-step.v1")
    );
    assert_eq!(value["decision"].as_str(), Some("build_next_cycle_input"));
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
        value["next_cycle_input"]["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );
    assert!(value["next_cycle_input"]["evaluation"]["fitness"].is_null());
    assert!(value["next_cycle_input"].get("fitness_report").is_none());
    assert!(
        value["next_cycle_input"]["design"]
            .as_str()
            .unwrap()
            .contains("missing route assertion")
    );

    let root_entries = fs::read_dir(&root)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        root_entries.len(),
        3,
        "retry-step must be stdout-only and not create hidden durable files"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn senior_swe_bench_retry_step_inspects_passed_evidence_and_rejects_mismatch() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-retry-step-pass-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let retry_plan = root.join("retry-plan.json");
    let cycle_input = root.join("cycle-input.json");
    let evaluation = root.join("local-evaluation.json");
    fs::write(&retry_plan, sample_retry_plan()).unwrap();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(
        &evaluation,
        r#"{
  "schema_version": "a2d.senior-swe-bench-local-evaluation.v1",
  "task_id": "task-hard",
  "repo": "owner/repo",
  "evaluator": "provided_local_command",
  "status": "passed",
  "exit_code": 0,
  "candidate_patch_hash": "abc12300",
  "github_solution_search_allowed": false,
  "fitness_evidence_path": "runs/example/fitness.json"
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-step",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "1",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--local-evaluation",
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry step command");
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["decision"].as_str(), Some("inspect_fitness_evidence"));
    assert_eq!(
        value["fitness_evidence_inspect_args"].as_array().unwrap()[0].as_str(),
        Some("fitness-evidence-inspect")
    );
    assert_eq!(
        value["fitness_evidence_inspect_args"].as_array().unwrap()[2].as_str(),
        Some("--require-all-tests-pass")
    );

    let mismatched_plan = root.join("mismatched-plan.json");
    fs::write(
        &mismatched_plan,
        sample_retry_plan().replace("\"task-hard\"", "\"other-task\""),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-step",
            "--retry-plan",
            mismatched_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--local-evaluation",
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry step command");
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("does not match"));

    let malformed_plan = root.join("malformed-plan.json");
    let mut malformed: serde_json::Value = serde_json::from_str(sample_retry_plan()).unwrap();
    malformed["stop_criteria"] = serde_json::json!([]);
    malformed["information_barriers"]["official_hidden_holdout_output_to_coder"] =
        serde_json::Value::Null;
    malformed["attempts"][0]["required_gates"] = serde_json::json!([null]);
    fs::write(
        &malformed_plan,
        serde_json::to_string_pretty(&malformed).unwrap(),
    )
    .unwrap();
    let malformed_rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-step",
            "--retry-plan",
            malformed_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--local-evaluation",
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run retry step command");
    assert_eq!(malformed_rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&malformed_rejected.stderr).contains("stop_criteria"));

    let _ = fs::remove_dir_all(root);
}
