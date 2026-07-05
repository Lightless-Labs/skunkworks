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

fn sample_retry_plan(max_attempts: usize) -> String {
    let attempts = (0..max_attempts)
        .map(|attempt| {
            let failed = if attempt + 1 < max_attempts {
                "build_next_cycle_input_with_senior_swe_bench_cycle_input_feedback"
            } else {
                "stop_attempts_exhausted_without_fitness_claim"
            };
            serde_json::json!({
                "attempt_index": attempt,
                "cycle_input_source": if attempt == 0 { "initial_task_cycle_input" } else { "feedback_from_previous_local_evaluation" },
                "required_gates": ["run_cycle_input_with_output_artifacts", "extract_unified_diff_candidate_patch", "evaluate_candidate_patch_against_checkout", "inspect_a2d_fitness_evidence_when_evaluator_passes"],
                "on_patch_extraction_failure": "stop_fail_closed_without_evaluator_or_fitness_claim",
                "on_evaluation_passed": "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
                "on_evaluation_failed": failed
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({
      "schema_version": "a2d.senior-swe-bench-cycle-retry-plan.v1",
      "task_id": "task-hard",
      "repo": "owner/repo",
      "github_solution_search_allowed": false,
      "max_attempts": max_attempts,
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
      "attempts": attempts
    }))
    .unwrap()
}

fn write_manifest(root: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let artifact = root.join(format!("{name}.artifact"));
    fs::write(&artifact, bytes).unwrap();
    let manifest = root.join(format!("{name}-manifest.json"));
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

struct Fixture {
    root: std::path::PathBuf,
    retry_plan: std::path::PathBuf,
    cycle_input: std::path::PathBuf,
    checkout: std::path::PathBuf,
    work_dir: std::path::PathBuf,
    evaluator: std::path::PathBuf,
    manifest: std::path::PathBuf,
}

fn write_fixture(name: &str, max_attempts: usize, evaluator_body: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("a2d-retry-execute-{name}-{}", unique_suffix()));
    let checkout = root.join("checkout");
    let src = checkout.join("src");
    let work_dir = root.join("work");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&work_dir).unwrap();
    fs::write(src.join("lib.rs"), "old\n").unwrap();
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&checkout)
        .status()
        .expect("git init");
    let retry_plan = root.join("retry-plan.json");
    fs::write(&retry_plan, sample_retry_plan(max_attempts)).unwrap();
    let cycle_input = root.join("cycle-input.json");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    let evaluator = root.join("evaluate.sh");
    fs::write(
        &evaluator,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\n{}\n",
            evaluator_body
        ),
    )
    .unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(&evaluator)
        .status()
        .expect("chmod evaluator");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let manifest = write_manifest(&root, "candidate", diff);
    Fixture {
        root,
        retry_plan,
        cycle_input,
        checkout,
        work_dir,
        evaluator,
        manifest,
    }
}

#[test]
fn retry_execute_succeeds_on_first_precomputed_attempt_without_provider_invocation() {
    let fixture = write_fixture(
        "success",
        2,
        "test \"${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}\" = true\ngrep -q new src/lib.rs\n",
    );
    let evidence_dir = fixture.root.join("fitness");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &evidence_dir)
        .args([
            "senior-swe-bench-retry-execute",
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--task-cycle-input",
            fixture.cycle_input.to_str().unwrap(),
            "--checkout",
            fixture.checkout.to_str().unwrap(),
            "--work-dir",
            fixture.work_dir.to_str().unwrap(),
            "--attempt-output-manifest",
            fixture.manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-execution.v1")
    );
    assert_eq!(value["status"].as_str(), Some("success"));
    assert_eq!(value["attempts_executed"].as_u64(), Some(1));
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));
    assert_eq!(value["evaluator_invocations_started"].as_bool(), Some(true));
    assert_eq!(
        value["terminal_run_result"]["official_senior_swe_bench_mastery"].as_bool(),
        Some(false)
    );
    assert!(value["final_evidence_path"].as_str().is_some());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_stops_after_attempt_exhaustion_without_evidence_claim() {
    let fixture = write_fixture("exhausted", 1, "echo public failure >&2\nexit 1\n");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-execute",
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--task-cycle-input",
            fixture.cycle_input.to_str().unwrap(),
            "--checkout",
            fixture.checkout.to_str().unwrap(),
            "--work-dir",
            fixture.work_dir.to_str().unwrap(),
            "--attempt-output-manifest",
            fixture.manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"].as_str(), Some("failed"));
    assert_eq!(
        value["stop_reason"].as_str(),
        Some("max_attempts_exhausted")
    );
    assert_eq!(
        value["fitness_claim_allowed_after_evidence_inspection"].as_bool(),
        Some(false)
    );
    assert!(value.get("terminal_run_result").is_none());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_reports_precomputed_manifest_exhaustion_before_max_attempts() {
    let fixture = write_fixture(
        "precomputed-exhausted",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-execute",
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--task-cycle-input",
            fixture.cycle_input.to_str().unwrap(),
            "--checkout",
            fixture.checkout.to_str().unwrap(),
            "--work-dir",
            fixture.work_dir.to_str().unwrap(),
            "--attempt-output-manifest",
            fixture.manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"].as_str(), Some("failed"));
    assert_eq!(
        value["stop_reason"].as_str(),
        Some("precomputed_attempt_manifests_exhausted")
    );
    assert_eq!(value["attempts_executed"].as_u64(), Some(1));
    assert_eq!(value["evaluator_invocations_started"].as_bool(), Some(true));
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_rejects_more_precomputed_manifests_than_bounded_attempts() {
    let fixture = write_fixture("too-many", 1, "grep -q new src/lib.rs\n");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-execute",
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--task-cycle-input",
            fixture.cycle_input.to_str().unwrap(),
            "--checkout",
            fixture.checkout.to_str().unwrap(),
            "--work-dir",
            fixture.work_dir.to_str().unwrap(),
            "--attempt-output-manifest",
            fixture.manifest.to_str().unwrap(),
            "--attempt-output-manifest",
            fixture.manifest.to_str().unwrap(),
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("more precomputed attempt manifests"));

    let _ = fs::remove_dir_all(fixture.root);
}
