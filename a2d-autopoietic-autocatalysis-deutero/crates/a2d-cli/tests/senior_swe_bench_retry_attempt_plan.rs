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
  "success_requires": ["a2d.fitness-evidence.v1", "actual_tests_evaluated:true", "non_regressing:true", "all_tests_pass:true"],
  "stop_criteria": ["candidate_patch_extraction_failed", "evaluation_passed_with_valid_fitness_evidence", "evaluation_rejected_for_policy_or_binding_mismatch", "max_attempts_exhausted"],
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
  ]
}
"#
}

fn write_manifest(
    root: &std::path::Path,
    artifact: &std::path::Path,
    bytes: &[u8],
) -> std::path::PathBuf {
    let manifest = root.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.cycle-output-artifacts.v1",
            "artifacts": [{
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": git_hash_object_bytes(bytes),
                "bytes": bytes.len()
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    manifest
}

#[test]
fn retry_attempt_plan_composes_selection_extraction_evaluation_and_retry_step_args() {
    let root = std::env::temp_dir().join(format!("a2d-retry-attempt-plan-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let retry_plan = root.join("retry-plan.json");
    let cycle_input = root.join("cycle-input.json");
    let checkout = root.join("checkout");
    let attempt_dir = root.join("attempt-0");
    let official_manifest = root.join("official-manifest.json");
    fs::create_dir_all(&checkout).unwrap();
    fs::write(&retry_plan, sample_retry_plan()).unwrap();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(&official_manifest, "{}\n").unwrap();
    let artifact = root.join("candidate.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&artifact, diff).unwrap();
    let manifest = write_manifest(&root, &artifact, diff);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--apply-candidate-patch",
            "--official-evaluator-manifest",
            official_manifest.to_str().unwrap(),
            "--",
            "./evaluate.sh",
            "--flag",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-plan.v1")
    );
    assert_eq!(
        value["decision"].as_str(),
        Some("extract_and_evaluate_candidate_patch")
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
        value["selected_artifact"]["path"].as_str(),
        Some(artifact.to_str().unwrap())
    );
    assert!(
        value["evaluate_args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.as_str() == Some("--apply-candidate-patch"))
    );
    assert!(
        value["evaluate_args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.as_str() == Some("--official-evaluator-manifest"))
    );
    assert_eq!(
        value["retry_step_args"].as_array().unwrap()[0].as_str(),
        Some("senior-swe-bench-retry-step")
    );
    assert!(
        !attempt_dir.exists(),
        "planning command must not write attempt output dirs"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn retry_attempt_plan_stops_on_non_extractable_artifact_and_rejects_unsafe_inputs() {
    let root = std::env::temp_dir().join(format!("a2d-retry-attempt-plan-bad-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let retry_plan = root.join("retry-plan.json");
    let cycle_input = root.join("cycle-input.json");
    let checkout = root.join("checkout");
    fs::create_dir_all(&checkout).unwrap();
    fs::write(&retry_plan, sample_retry_plan()).unwrap();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    let prose_artifact = root.join("prose.artifact");
    let prose = b"I'll inspect the local checkout first.";
    fs::write(&prose_artifact, prose).unwrap();
    let manifest = write_manifest(&root, &prose_artifact, prose);
    let attempt_dir = root.join("attempt-0");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["decision"].as_str(), Some("stop"));
    assert_eq!(
        value["stop_reason"].as_str(),
        Some("candidate_patch_extraction_failed")
    );
    assert!(value.get("evaluate_args").is_none());

    let missing_command = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(missing_command.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing_command.stderr).contains("evaluator command is empty")
    );

    let multi_stdin_rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            "-",
            "--attempt-index",
            "0",
            "--task-cycle-input",
            "-",
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(multi_stdin_rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&multi_stdin_rejected.stderr).contains("at most one"));

    let missing_official_manifest = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--official-evaluator-manifest",
            root.join("missing-official.json").to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(missing_official_manifest.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing_official_manifest.stderr)
            .contains("official evaluator manifest not found")
    );

    let out_of_range = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "2",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(out_of_range.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out_of_range.stderr).contains("outside max_attempts"));

    let public_artifact = root.join("public.artifact");
    let public = b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\nhttps://github.com/owner/repo/pull/1\n";
    fs::write(&public_artifact, public).unwrap();
    let public_manifest = write_manifest(&root, &public_artifact, public);
    let public_rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            public_manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(public_rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&public_rejected.stderr).contains("public GitHub"));

    let multi_manifest = root.join("multi-manifest.json");
    fs::write(
        &multi_manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.cycle-output-artifacts.v1",
            "artifacts": [
                {
                    "cycle": 0,
                    "report_cycle": 1,
                    "workcell_id": "wc-0001",
                    "enzyme_id": "coder",
                    "provider": "test-provider",
                    "artifact_type": "code",
                    "path": prose_artifact,
                    "git_object_hash": git_hash_object_bytes(prose),
                    "bytes": prose.len()
                },
                {
                    "cycle": 0,
                    "report_cycle": 1,
                    "workcell_id": "wc-0002",
                    "enzyme_id": "coder",
                    "provider": "test-provider",
                    "artifact_type": "code",
                    "path": public_artifact,
                    "git_object_hash": git_hash_object_bytes(public),
                    "bytes": public.len()
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let multi_rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            retry_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            multi_manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(multi_rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&multi_rejected.stderr).contains("expected exactly one"));

    let mismatched_plan = root.join("mismatched-plan.json");
    fs::write(
        &mismatched_plan,
        sample_retry_plan().replace("\"task-hard\"", "\"other-task\""),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-plan",
            "--retry-plan",
            mismatched_plan.to_str().unwrap(),
            "--attempt-index",
            "0",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--cycle-output-manifest",
            manifest.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--attempt-dir",
            attempt_dir.to_str().unwrap(),
            "--",
            "./evaluate.sh",
        ])
        .output()
        .expect("run retry attempt plan command");
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("does not match"));

    let _ = fs::remove_dir_all(root);
}
