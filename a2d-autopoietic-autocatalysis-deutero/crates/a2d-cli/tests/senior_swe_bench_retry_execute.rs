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

fn write_retry_next_cycle_execution(
    fixture: &Fixture,
    status: &str,
    stop_reason: &str,
    manifest: &std::path::Path,
) -> std::path::PathBuf {
    let retry_execution_path = fixture.work_dir.join("retry-execution.json");
    let retry_execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution_path).unwrap()).unwrap();
    let next_cycle_command = retry_execution["next_cycle_command"].clone();
    let path = fixture
        .work_dir
        .join("attempt-1/retry-next-cycle-execution.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-next-cycle-execution.v1",
            "status": status,
            "stop_reason": stop_reason,
            "task_id": "task-hard",
            "repo": "owner/repo",
            "attempt_index": 1,
            "retry_execution_path": retry_execution_path,
            "next_cycle_command": next_cycle_command,
            "task_cycle_input": fixture.work_dir.join("attempt-0/next-cycle-input.json"),
            "checkout": fixture.checkout,
            "output_artifacts_dir": fixture.work_dir.join("attempt-1/cycle-output-artifacts"),
            "cycle_output_manifest": manifest,
            "cycle_output_manifest_git_object_hash": git_hash_object_bytes(&fs::read(manifest).unwrap()),
            "cycle_output_artifact_count": 1,
            "cycle_input_command_started": status == "success",
            "cycle_input_command_spawned": status == "success",
            "cycle_input_command_timed_out": false,
            "cycle_input_spawn_error": serde_json::Value::Null,
            "cycle_input_exit_code": if status == "success" { serde_json::json!(0) } else { serde_json::json!(1) },
            "cycle_input_stdout_preview": "",
            "cycle_input_stderr_preview": "",
            "provider_invocations_started_by_this_command": status == "success",
            "evaluator_invocations_started": false,
            "fitness_evidence_inspection_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "fitness_claim_allowed_after_cycle": false,
            "github_solution_search_allowed": false,
            "note": "test retry next-cycle execution summary"
        }))
        .unwrap(),
    )
    .unwrap();
    path
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

    let attempt_dir = fixture.work_dir.join("attempt-0");
    let expected_artifacts = [
        (
            attempt_dir.join("retry-attempt-plan.json"),
            "a2d.senior-swe-bench-retry-attempt-plan.v1",
        ),
        (
            attempt_dir.join("retry-attempt-extraction.json"),
            "a2d.senior-swe-bench-retry-attempt-extraction.v1",
        ),
        (
            attempt_dir.join("retry-attempt-evaluation.json"),
            "a2d.senior-swe-bench-retry-attempt-evaluation.v1",
        ),
        (
            attempt_dir.join("retry-attempt-step-execution.json"),
            "a2d.senior-swe-bench-retry-attempt-step-execution.v1",
        ),
        (
            attempt_dir.join("retry-attempt-step-evidence-execution.json"),
            "a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1",
        ),
        (
            attempt_dir.join("retry-run-result.json"),
            "a2d.senior-swe-bench-retry-run-result.v1",
        ),
        (
            fixture.work_dir.join("retry-execution.json"),
            "a2d.senior-swe-bench-retry-execution.v1",
        ),
    ];
    for (path, schema) in expected_artifacts {
        let artifact: serde_json::Value = serde_json::from_slice(
            &fs::read(&path).unwrap_or_else(|error| panic!("missing {}: {error}", path.display())),
        )
        .unwrap();
        assert_eq!(artifact["schema_version"].as_str(), Some(schema));
    }

    let persisted_execution: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.work_dir.join("retry-execution.json")).unwrap())
            .unwrap();
    assert_eq!(persisted_execution, value);

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
    let persisted_execution: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.work_dir.join("retry-execution.json")).unwrap())
            .unwrap();
    assert_eq!(persisted_execution, value);
    let attempt_dir = fixture.work_dir.join("attempt-0");
    for (name, schema) in [
        (
            "retry-attempt-plan.json",
            "a2d.senior-swe-bench-retry-attempt-plan.v1",
        ),
        (
            "retry-attempt-extraction.json",
            "a2d.senior-swe-bench-retry-attempt-extraction.v1",
        ),
        (
            "retry-attempt-evaluation.json",
            "a2d.senior-swe-bench-retry-attempt-evaluation.v1",
        ),
        (
            "retry-attempt-step-execution.json",
            "a2d.senior-swe-bench-retry-attempt-step-execution.v1",
        ),
    ] {
        let artifact: serde_json::Value =
            serde_json::from_slice(&fs::read(attempt_dir.join(name)).unwrap()).unwrap();
        assert_eq!(artifact["schema_version"].as_str(), Some(schema));
    }
    assert!(
        !attempt_dir
            .join("retry-attempt-step-evidence-execution.json")
            .exists()
    );
    assert!(!attempt_dir.join("retry-run-result.json").exists());

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
    let next_cycle_input_path = fixture.work_dir.join("attempt-0/next-cycle-input.json");
    let next_cycle_output_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    assert_eq!(
        value["attempts"][0]["next_cycle_command"]["argv"],
        serde_json::json!([
            "cycle-input",
            next_cycle_input_path.to_string_lossy(),
            "1",
            "--checkout",
            fixture.checkout.to_string_lossy(),
            "--output-artifacts",
            next_cycle_output_dir.to_string_lossy(),
        ])
    );
    assert_eq!(
        value["attempts"][0]["next_cycle_command"]["expected_manifest_path"],
        serde_json::json!(
            next_cycle_output_dir
                .join("manifest.json")
                .to_string_lossy()
        )
    );
    assert_eq!(
        value["attempts"][0]["next_cycle_command"]["provider_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["attempts"][0]["next_cycle_command"]["fitness_claim_allowed_before_evidence"]
            .as_bool(),
        Some(false)
    );
    assert_eq!(
        value["next_cycle_command"],
        value["attempts"][0]["next_cycle_command"]
    );
    let persisted_execution: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.work_dir.join("retry-execution.json")).unwrap())
            .unwrap();
    assert_eq!(persisted_execution, value);
    let next_cycle_input: serde_json::Value =
        serde_json::from_slice(&fs::read(&next_cycle_input_path).unwrap()).unwrap();
    assert_eq!(
        next_cycle_input["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_rejects_stale_next_cycle_input_before_overwrite() {
    let fixture = write_fixture("stale-next-cycle", 2, "echo public failure >&2\nexit 1\n");
    fs::create_dir_all(fixture.work_dir.join("attempt-0")).unwrap();
    fs::write(
        fixture.work_dir.join("attempt-0/next-cycle-input.json"),
        "{\"stale\":true}\n",
    )
    .unwrap();
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
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("artifact already exists"));
    let stale: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.work_dir.join("attempt-0/next-cycle-input.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stale["stale"].as_bool(), Some(true));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_consumes_persisted_next_cycle_boundary() {
    let fixture = write_fixture("resume-attempt", 2, "echo public failure >&2\nexit 1\n");
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let next_manifest_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    fs::create_dir_all(&next_manifest_dir).unwrap();
    let generated_manifest = write_manifest(
        &next_manifest_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let next_manifest = next_manifest_dir.join("manifest.json");
    fs::rename(&generated_manifest, &next_manifest).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--retry-execution",
            fixture
                .work_dir
                .join("retry-execution.json")
                .to_str()
                .unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--cycle-output-manifest",
            next_manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan");
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
        Some("a2d.senior-swe-bench-retry-attempt-plan.v1")
    );
    assert_eq!(value["attempt_index"].as_u64(), Some(1));
    assert_eq!(
        value["task_cycle_input"],
        serde_json::json!(
            fixture
                .work_dir
                .join("attempt-0/next-cycle-input.json")
                .to_string_lossy()
        )
    );
    assert_eq!(
        value["cycle_output_manifest"],
        serde_json::json!(next_manifest.to_string_lossy())
    );
    assert_eq!(
        value["attempt_dir"],
        serde_json::json!(fixture.work_dir.join("attempt-1").to_string_lossy())
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

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_consumes_successful_next_cycle_execution_summary() {
    let fixture = write_fixture(
        "resume-next-cycle-summary",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let next_manifest_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    fs::create_dir_all(&next_manifest_dir).unwrap();
    let generated_manifest = write_manifest(
        &next_manifest_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let next_manifest = next_manifest_dir.join("manifest.json");
    fs::rename(&generated_manifest, &next_manifest).unwrap();
    let next_cycle_execution = write_retry_next_cycle_execution(
        &fixture,
        "success",
        "cycle_output_manifest_ready",
        &next_manifest,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--next-cycle-execution",
            next_cycle_execution.to_str().unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan from next-cycle summary");
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
        Some("a2d.senior-swe-bench-retry-attempt-plan.v1")
    );
    assert_eq!(value["attempt_index"].as_u64(), Some(1));
    assert_eq!(
        value["cycle_output_manifest"],
        serde_json::json!(next_manifest.to_string_lossy())
    );
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_rejects_stale_next_cycle_execution_manifest_hash() {
    let fixture = write_fixture(
        "resume-stale-next-cycle-summary",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let next_manifest_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    fs::create_dir_all(&next_manifest_dir).unwrap();
    let generated_manifest = write_manifest(
        &next_manifest_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let next_manifest = next_manifest_dir.join("manifest.json");
    fs::rename(&generated_manifest, &next_manifest).unwrap();
    let next_cycle_execution = write_retry_next_cycle_execution(
        &fixture,
        "success",
        "cycle_output_manifest_ready",
        &next_manifest,
    );
    let replacement_manifest = write_manifest(
        &next_manifest_dir,
        "replacement",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+other\n",
    );
    fs::rename(&replacement_manifest, &next_manifest).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--next-cycle-execution",
            next_cycle_execution.to_str().unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan from stale next-cycle summary");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest hash"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_rejects_next_cycle_execution_fitness_claim_fields() {
    let fixture = write_fixture(
        "resume-fitness-claim-summary",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let next_manifest_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    fs::create_dir_all(&next_manifest_dir).unwrap();
    let generated_manifest = write_manifest(
        &next_manifest_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let next_manifest = next_manifest_dir.join("manifest.json");
    fs::rename(&generated_manifest, &next_manifest).unwrap();
    let next_cycle_execution = write_retry_next_cycle_execution(
        &fixture,
        "success",
        "cycle_output_manifest_ready",
        &next_manifest,
    );
    let mut summary: serde_json::Value =
        serde_json::from_slice(&fs::read(&next_cycle_execution).unwrap()).unwrap();
    summary["fitness"] = serde_json::json!(1.0);
    fs::write(
        &next_cycle_execution,
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--next-cycle-execution",
            next_cycle_execution.to_str().unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan from claim-bearing next-cycle summary");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("pre-evidence fitness claim"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_rejects_failed_next_cycle_execution_summary() {
    let fixture = write_fixture(
        "resume-failed-next-cycle-summary",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let next_manifest_dir = fixture.work_dir.join("attempt-1/cycle-output-artifacts");
    fs::create_dir_all(&next_manifest_dir).unwrap();
    let generated_manifest = write_manifest(
        &next_manifest_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let next_manifest = next_manifest_dir.join("manifest.json");
    fs::rename(&generated_manifest, &next_manifest).unwrap();
    let next_cycle_execution = write_retry_next_cycle_execution(
        &fixture,
        "failed",
        "cycle_input_command_failed",
        &next_manifest,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--next-cycle-execution",
            next_cycle_execution.to_str().unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan from failed next-cycle summary");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("requires successful retry next-cycle execution")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_rejects_manifest_that_does_not_match_next_command() {
    let fixture = write_fixture(
        "resume-wrong-manifest",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let wrong_manifest = write_manifest(
        &fixture.root,
        "wrong-next",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--retry-execution",
            fixture
                .work_dir
                .join("retry-execution.json")
                .to_str()
                .unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--cycle-output-manifest",
            wrong_manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not match expected manifest path")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_plan_rejects_inconsistent_next_attempt_dir() {
    let fixture = write_fixture(
        "resume-wrong-attempt-dir",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run first retry execute");
    assert_eq!(first.status.code(), Some(2));

    let wrong_output_dir = fixture.work_dir.join("attempt-2/cycle-output-artifacts");
    fs::create_dir_all(&wrong_output_dir).unwrap();
    let generated_manifest = write_manifest(
        &wrong_output_dir,
        "candidate",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let wrong_manifest = wrong_output_dir.join("manifest.json");
    fs::rename(&generated_manifest, &wrong_manifest).unwrap();

    let retry_execution_path = fixture.work_dir.join("retry-execution.json");
    let mut retry_execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution_path).unwrap()).unwrap();
    let command = serde_json::json!({
        "command": "a2d",
        "argv": [
            "cycle-input",
            fixture.work_dir.join("attempt-0/next-cycle-input.json").to_string_lossy(),
            "1",
            "--checkout",
            fixture.checkout.to_string_lossy(),
            "--output-artifacts",
            wrong_output_dir.to_string_lossy(),
        ],
        "expected_manifest_path": wrong_manifest.to_string_lossy(),
        "provider_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
    });
    retry_execution["next_cycle_command"] = command.clone();
    retry_execution["attempts"][0]["next_cycle_command"] = command;
    fs::write(
        &retry_execution_path,
        serde_json::to_vec_pretty(&retry_execution).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-resume-attempt-plan",
            "--retry-execution",
            retry_execution_path.to_str().unwrap(),
            "--retry-plan",
            fixture.retry_plan.to_str().unwrap(),
            "--cycle-output-manifest",
            wrong_manifest.to_str().unwrap(),
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry resume attempt plan");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match expected"));

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
