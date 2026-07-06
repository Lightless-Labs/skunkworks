use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("a2d-cli is under project crates directory")
        .to_path_buf()
}

fn project_relative(path: &Path) -> String {
    path.strip_prefix(project_root())
        .expect("path under project root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn assert_json_contains_no_host_absolute_paths(value: &serde_json::Value, forbidden: &[String]) {
    match value {
        serde_json::Value::String(text) => {
            for prefix in forbidden {
                assert!(
                    !text.contains(prefix),
                    "JSON string contains host-local absolute path {prefix}: {text}"
                );
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_json_contains_no_host_absolute_paths(item, forbidden);
            }
        }
        serde_json::Value::Object(object) => {
            for item in object.values() {
                assert_json_contains_no_host_absolute_paths(item, forbidden);
            }
        }
        _ => {}
    }
}

fn git_hash_object_bytes(bytes: &[u8]) -> String {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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

fn write_executable_script(path: &std::path::Path, body: &str) {
    fs::write(
        path,
        format!("#!/usr/bin/env bash\nset -euo pipefail\n{}\n", body),
    )
    .unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(path)
        .status()
        .expect("chmod script");
}

fn write_official_manifest(
    root: &std::path::Path,
    evaluator: &std::path::Path,
) -> std::path::PathBuf {
    let manifest = root.join("official-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
            "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_provided_command": [evaluator.to_string_lossy()]
        }))
        .unwrap(),
    )
    .unwrap();
    manifest
}

fn write_forged_official_manifest_inspection(
    fixture: &Fixture,
    manifest: &std::path::Path,
) -> std::path::PathBuf {
    let hash = git_hash_object_bytes(&fs::read(manifest).unwrap());
    let path = fixture
        .root
        .join("forged-official-manifest-inspection.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "official_evaluator_manifest_path": manifest,
            "official_evaluator_manifest_hash": hash,
            "official_benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
            "official_hidden_holdouts": true,
            "official_github_solution_search_allowed": false,
            "official_benchmark_provided_command": [fixture.evaluator.to_string_lossy()],
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "fitness_evidence_inspection_started": false,
            "github_solution_search_allowed": false,
            "fitness_claim_allowed_before_evidence": false,
            "official_senior_swe_bench_mastery": false
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn write_official_manifest_inspection(
    fixture: &Fixture,
    manifest: &std::path::Path,
) -> std::path::PathBuf {
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-official-evaluator-manifest-inspect",
            "--task-cycle-input",
            fixture.cycle_input.to_str().unwrap(),
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run official manifest inspect");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = fixture.root.join("official-manifest-inspection.json");
    fs::write(&path, output.stdout).unwrap();
    path
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
    write_fixture_in(std::env::temp_dir(), name, max_attempts, evaluator_body)
}

fn write_fixture_in(
    parent: PathBuf,
    name: &str,
    max_attempts: usize,
    evaluator_body: &str,
) -> Fixture {
    let root = parent.join(format!("a2d-retry-execute-{name}-{}", unique_suffix()));
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
    write_executable_script(&evaluator, evaluator_body);
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
fn retry_execute_persists_official_manifest_inspection_before_evaluator() {
    let fixture = write_fixture(
        "official-inspection",
        2,
        "test \"${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}\" = true\ngrep -q new src/lib.rs\n",
    );
    let official_manifest = write_official_manifest(&fixture.root, &fixture.evaluator);
    let inspection = write_official_manifest_inspection(&fixture, &official_manifest);
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
            "--official-evaluator-manifest",
            official_manifest.to_str().unwrap(),
            "--official-evaluator-manifest-inspection",
            inspection.to_str().unwrap(),
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
    let persisted_inspection = fixture
        .work_dir
        .join("official-evaluator-manifest-inspection.json");
    assert_eq!(
        value["official_evaluator_manifest_inspection_path"].as_str(),
        Some(persisted_inspection.to_string_lossy().as_ref())
    );
    assert_eq!(
        value["official_evaluator_manifest_inspection_validated"].as_bool(),
        Some(true)
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(&persisted_inspection).unwrap()).unwrap();
    assert_eq!(
        persisted["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1")
    );
    assert_eq!(
        persisted["official_evaluator_manifest_path"].as_str(),
        Some(official_manifest.to_string_lossy().as_ref())
    );
    assert_eq!(
        persisted["evaluator_invocations_started"].as_bool(),
        Some(false)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_rejects_noncanonical_official_manifest_inspection_before_evaluator() {
    let fixture = write_fixture(
        "official-noncanonical-inspection",
        2,
        "touch evaluator-ran\n",
    );
    let official_manifest = write_official_manifest(&fixture.root, &fixture.evaluator);
    let forged_inspection = write_forged_official_manifest_inspection(&fixture, &official_manifest);

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
            "--official-evaluator-manifest",
            official_manifest.to_str().unwrap(),
            "--official-evaluator-manifest-inspection",
            forged_inspection.to_str().unwrap(),
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("note"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !fixture.checkout.join("evaluator-ran").exists(),
        "noncanonical inspection must fail before evaluator execution"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_requires_official_manifest_inspection_with_official_manifest() {
    let fixture = write_fixture("official-missing-inspection", 2, "touch evaluator-ran\n");
    let official_manifest = write_official_manifest(&fixture.root, &fixture.evaluator);

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
            "--official-evaluator-manifest",
            official_manifest.to_str().unwrap(),
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("official-evaluator-manifest-inspection"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !fixture.checkout.join("evaluator-ran").exists(),
        "missing inspection must fail before evaluator execution"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_execute_rejects_forged_official_manifest_inspection() {
    let fixture = write_fixture("official-forged-inspection", 2, "touch evaluator-ran\n");
    let official_manifest = write_official_manifest(&fixture.root, &fixture.evaluator);
    let mut manifest_value: serde_json::Value =
        serde_json::from_slice(&fs::read(&official_manifest).unwrap()).unwrap();
    manifest_value["hidden_holdouts"] = serde_json::json!(false);
    fs::write(
        &official_manifest,
        serde_json::to_vec_pretty(&manifest_value).unwrap(),
    )
    .unwrap();
    let forged_inspection = write_forged_official_manifest_inspection(&fixture, &official_manifest);

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
            "--official-evaluator-manifest",
            official_manifest.to_str().unwrap(),
            "--official-evaluator-manifest-inspection",
            forged_inspection.to_str().unwrap(),
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("hidden_holdouts"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !fixture.checkout.join("evaluator-ran").exists(),
        "forged inspection must fail before evaluator execution"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_validates_success_evidence_before_allowing_claim() {
    let fixture = write_fixture(
        "status-success",
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
    assert_eq!(output.status.code(), Some(0));

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(
        status_output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout).unwrap();
    assert_eq!(
        status["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-status.v1")
    );
    assert_eq!(status["next_action"].as_str(), Some("completed_success"));
    assert_eq!(
        status["fitness_claim_allowed_by_status"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status["fitness_evidence_validated_by_status"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status["authoritative_evidence_gate"].as_str(),
        Some("fitness-evidence-inspect --require-all-tests-pass")
    );
    assert_eq!(
        status["fitness_evidence_inspection_performed_by_status"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status["official_senior_swe_bench_mastery"].as_bool(),
        Some(false)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_regressing_success_evidence() {
    let fixture = write_fixture(
        "status-regressing-evidence",
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
    assert_eq!(output.status.code(), Some(0));
    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    let final_evidence_path = execution["final_evidence_path"].as_str().unwrap();
    let mut evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(final_evidence_path).unwrap()).unwrap();
    evidence["non_regressing"] = serde_json::json!(false);
    fs::write(
        final_evidence_path,
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains("regressing"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_tampered_official_mastery_claim() {
    let fixture = write_fixture(
        "status-official-tamper",
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
    assert_eq!(output.status.code(), Some(0));
    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let mut execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    execution["final_evaluator_kind"] = serde_json::json!("official_senior_swe_bench");
    execution["official_senior_swe_bench_mastery"] = serde_json::json!(true);
    execution["terminal_run_result"]["final_evaluator_kind"] =
        serde_json::json!("official_senior_swe_bench");
    execution["terminal_run_result"]["official_senior_swe_bench_mastery"] = serde_json::json!(true);
    fs::write(
        &retry_execution,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains("evaluator_kind"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_stale_terminal_evidence_summary() {
    let fixture = write_fixture(
        "status-summary-tamper",
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
    assert_eq!(output.status.code(), Some(0));
    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let mut execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    execution["terminal_run_result"]["fitness_evidence_summary"]["fitness"] =
        serde_json::json!(0.5);
    fs::write(
        &retry_execution,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains("fitness_evidence_summary"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_success_evidence_without_all_tests_pass() {
    let fixture = write_fixture(
        "status-failed-all-tests-evidence",
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
    assert_eq!(output.status.code(), Some(0));
    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    let final_evidence_path = execution["final_evidence_path"].as_str().unwrap();
    let mut evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(final_evidence_path).unwrap()).unwrap();
    let results = evidence["results"].as_array_mut().unwrap();
    let all_tests_pass = results
        .iter_mut()
        .find(|result| result["name"].as_str() == Some("all_tests_pass"))
        .expect("all_tests_pass result");
    all_tests_pass["passed"] = serde_json::json!(false);
    fs::write(
        final_evidence_path,
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains("all_tests_pass"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_failed_boundary_with_embedded_fitness_claims() {
    let fixture = write_fixture(
        "status-failed-claim-tamper",
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

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let mut execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    execution["terminal_run_result"] = serde_json::json!({
        "status": "success",
        "final_evidence_path": fixture.root.join("fake-evidence.json"),
        "official_senior_swe_bench_mastery": true
    });
    execution["final_evidence_path"] = serde_json::json!(fixture.root.join("fake-evidence.json"));
    execution["official_senior_swe_bench_mastery"] = serde_json::json!(true);
    fs::write(
        &retry_execution,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains("pre-evidence fitness claim"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_nested_failed_boundary_fitness_claims() {
    let fixture = write_fixture(
        "status-nested-claim-tamper",
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

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let mut execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    execution["attempts"][0]["fitness_evidence_path"] =
        serde_json::json!(fixture.root.join("fake-evidence.json"));
    fs::write(
        &retry_execution,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr)
            .contains("attempts[0].fitness_evidence_path"),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_reports_next_cycle_boundary_without_fitness_claim() {
    let fixture = write_fixture("status-next-cycle", 2, "echo public failure >&2\nexit 1\n");
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

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(
        status_output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status_output.stdout).unwrap();
    assert_eq!(status["next_action"].as_str(), Some("run_next_cycle"));
    assert_eq!(
        status["next_gate_command"],
        serde_json::json!({
            "command": "a2d",
            "argv": [
                "senior-swe-bench-retry-run-next-cycle",
                "--retry-execution",
                retry_execution.to_string_lossy(),
            ],
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "fitness_evidence_inspection_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "github_solution_search_allowed": false,
            "retry_execution_path_binding": "repo_relative_paths_resolve_against_a2d_project_root",
            "note": "status handoff only; running this command may start exactly one bounded cycle-input provider boundary, but this status command has not started it",
        })
    );
    assert_eq!(
        status["fitness_claim_allowed_by_status"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status["fitness_evidence_inspection_performed_by_status"].as_bool(),
        Some(false)
    );
    assert_eq!(status["next_cycle_attempt_index"].as_u64(), Some(1));
    assert_eq!(
        status["next_cycle_command"],
        serde_json::from_slice::<serde_json::Value>(&fs::read(&retry_execution).unwrap()).unwrap()
            ["next_cycle_command"]
    );

    let _ = fs::remove_dir_all(fixture.root);
}

fn assert_retry_status_rejects_tampered_next_cycle_command_flag(
    flag: &str,
    tampered_value: serde_json::Value,
    expected_error: &str,
) {
    let fixture = write_fixture(
        &format!("status-next-cycle-tampered-{flag}"),
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

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let mut execution: serde_json::Value =
        serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
    execution["next_cycle_command"][flag] = tampered_value.clone();
    execution["attempts"][0]["next_cycle_command"][flag] = tampered_value;
    fs::write(
        &retry_execution,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();

    let status_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-status",
            retry_execution.to_str().unwrap(),
        ])
        .output()
        .expect("run retry status");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr).contains(expected_error),
        "stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_status_rejects_next_cycle_command_that_prestarts_evaluator() {
    assert_retry_status_rejects_tampered_next_cycle_command_flag(
        "evaluator_invocations_started",
        serde_json::json!(true),
        "next_cycle_command evaluator_invocations_started must be false",
    );
}

#[test]
fn retry_status_rejects_next_cycle_command_that_prestarts_evidence_inspection() {
    assert_retry_status_rejects_tampered_next_cycle_command_flag(
        "fitness_evidence_inspection_started",
        serde_json::json!(true),
        "next_cycle_command fitness_evidence_inspection_started must be false",
    );
}

#[test]
fn retry_status_rejects_next_cycle_command_that_allows_github_solution_search() {
    assert_retry_status_rejects_tampered_next_cycle_command_flag(
        "github_solution_search_allowed",
        serde_json::json!(true),
        "next_cycle_command github_solution_search_allowed must be false",
    );
}

#[test]
fn retry_status_rejects_next_cycle_command_non_boolean_boundary_flag() {
    assert_retry_status_rejects_tampered_next_cycle_command_flag(
        "evaluator_invocations_started",
        serde_json::json!("true"),
        "next_cycle_command evaluator_invocations_started must be false",
    );
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
fn retry_status_next_gate_paths_are_repo_relative_and_cwd_stable() {
    let fixture_parent = project_root().join("target/a2d-retry-cwd-stability");
    let fixture = write_fixture_in(
        fixture_parent,
        "repo-relative-next-gate",
        2,
        "echo public failure >&2\nexit 1\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(project_root())
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
        .expect("run retry execute from project root");
    assert_eq!(output.status.code(), Some(2));

    let retry_execution = fixture.work_dir.join("retry-execution.json");
    let retry_execution_rel = project_relative(&retry_execution);
    let status_from_root = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(project_root())
        .args(["senior-swe-bench-retry-status", &retry_execution_rel])
        .output()
        .expect("run retry status from project root");
    assert_eq!(
        status_from_root.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&status_from_root.stderr)
    );
    let status_from_subdir = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(project_root().join("crates/a2d-cli"))
        .args(["senior-swe-bench-retry-status", &retry_execution_rel])
        .output()
        .expect("run retry status from project subdir");
    assert_eq!(
        status_from_subdir.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&status_from_subdir.stderr)
    );

    let status_root: serde_json::Value = serde_json::from_slice(&status_from_root.stdout).unwrap();
    let status_subdir: serde_json::Value =
        serde_json::from_slice(&status_from_subdir.stdout).unwrap();
    assert_eq!(status_root, status_subdir);
    assert_json_contains_no_host_absolute_paths(
        &status_root,
        &[
            project_root().to_string_lossy().to_string(),
            fixture.root.to_string_lossy().to_string(),
        ],
    );
    assert_eq!(
        status_root["next_gate_command"]["argv"],
        serde_json::json!([
            "senior-swe-bench-retry-run-next-cycle",
            "--retry-execution",
            retry_execution_rel,
        ])
    );
    assert_eq!(
        status_root["next_cycle_command"]["argv"],
        serde_json::json!([
            "cycle-input",
            project_relative(&fixture.work_dir.join("attempt-0/next-cycle-input.json")),
            "1",
            "--checkout",
            project_relative(&fixture.checkout),
            "--output-artifacts",
            project_relative(&fixture.work_dir.join("attempt-1/cycle-output-artifacts")),
        ])
    );
    assert_eq!(
        status_root["next_cycle_command"]["expected_manifest_path"],
        serde_json::json!(project_relative(
            &fixture
                .work_dir
                .join("attempt-1/cycle-output-artifacts/manifest.json")
        ))
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
fn retry_run_next_gate_plans_from_successful_next_cycle_summary_without_evaluator() {
    let fixture_parent = project_root().join("target/a2d-retry-next-gate-cwd-stability");
    let fixture = write_fixture_in(fixture_parent, "next-gate-resume-plan", 2, "exit 99\n");
    let evaluator_counter = fixture.root.join("evaluator-count");
    write_executable_script(
        &fixture.evaluator,
        &format!(
            "count=0\nif test -f {counter}; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nexit 1\n",
            counter = evaluator_counter.to_string_lossy()
        ),
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
    assert_eq!(
        fs::read_to_string(&evaluator_counter).unwrap(),
        "1",
        "setup should run the first-attempt evaluator exactly once"
    );

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

    let next_cycle_execution_rel = project_relative(&next_cycle_execution);
    let retry_plan_rel = project_relative(&fixture.retry_plan);
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(project_root().join("crates/a2d-cli"))
        .args([
            "senior-swe-bench-retry-run-next-gate",
            "--next-cycle-execution",
            &next_cycle_execution_rel,
            "--retry-plan",
            &retry_plan_rel,
            "--apply-candidate-patch",
            "--",
            fixture.evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run retry next-gate from subdir with repo-relative next-cycle summary");
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
        Some("a2d.senior-swe-bench-retry-next-gate-execution.v1")
    );
    assert_eq!(
        value["executed_gate"].as_str(),
        Some("retry_resume_attempt_plan")
    );
    assert_eq!(value["status"].as_str(), Some("success"));
    assert_eq!(
        value["child_schema"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-plan.v1")
    );
    assert_eq!(
        value["evaluator_invocations_started_by_this_command"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_evidence_inspection_started_by_this_command"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_claim_allowed_after_gate"].as_bool(),
        Some(false)
    );
    assert_eq!(value["child"]["attempt_index"].as_u64(), Some(1));
    assert_eq!(
        value["before_status"]["next_cycle_execution_path"].as_str(),
        Some(next_cycle_execution_rel.as_str())
    );
    assert_json_contains_no_host_absolute_paths(
        &value["child"]["resume_boundary"],
        &[
            project_root().to_string_lossy().to_string(),
            fixture.root.to_string_lossy().to_string(),
        ],
    );
    assert_eq!(
        value["child"]["resume_boundary"]["retry_execution_path"].as_str(),
        Some(project_relative(&fixture.work_dir.join("retry-execution.json")).as_str())
    );
    assert_eq!(
        value["child"]["resume_boundary"]["next_cycle_execution_path"].as_str(),
        Some(next_cycle_execution_rel.as_str())
    );
    assert!(
        fixture
            .work_dir
            .join("attempt-1/retry-attempt-plan.json")
            .is_file()
    );
    assert!(
        fixture
            .work_dir
            .join("attempt-1/retry-next-gate-resume-plan.json")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(&evaluator_counter).unwrap(),
        "1",
        "next-gate resume planning must not run evaluator side effects"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_runs_planned_attempt_and_persists_success_summary() {
    let fixture = write_fixture(
        "resume-attempt-execute-success",
        2,
        "echo overwritten by invocation-count evaluator >&2\nexit 99\n",
    );
    let counter = fixture.root.join("evaluator-count");
    write_executable_script(
        &fixture.evaluator,
        &format!(
            "count=0\nif test -f {counter}; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nif test \"$count\" = 1; then echo public first-attempt failure >&2; exit 1; fi\ngrep -q new src/lib.rs\n",
            counter = counter.to_string_lossy()
        ),
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
    let attempt0_evaluation_before = fs::read(
        fixture
            .work_dir
            .join("attempt-0/retry-attempt-evaluation.json"),
    )
    .unwrap();

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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
    assert_eq!(plan.status.code(), Some(0));

    let evidence_dir = fixture.root.join("fitness-resume");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &evidence_dir)
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
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
    assert_eq!(value["attempts_executed"].as_u64(), Some(2));
    assert_eq!(
        value["attempts"][0]["retry_step_decision"].as_str(),
        Some("build_next_cycle_input")
    );
    assert_eq!(
        value["attempts"][0]["fitness_evidence_inspection_passed"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["attempts"][1]["retry_step_decision"].as_str(),
        Some("inspect_fitness_evidence")
    );
    assert_eq!(
        value["attempts"][1]["fitness_evidence_inspection_passed"].as_bool(),
        Some(true)
    );
    assert_eq!(fs::read_to_string(&counter).unwrap(), "2");
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));
    assert_eq!(value["evaluator_invocations_started"].as_bool(), Some(true));
    assert!(value["final_evidence_path"].as_str().is_some());
    assert_eq!(
        value["terminal_run_result"]["official_senior_swe_bench_mastery"].as_bool(),
        Some(false)
    );

    for (relative, schema) in [
        (
            "attempt-1/retry-attempt-extraction.json",
            "a2d.senior-swe-bench-retry-attempt-extraction.v1",
        ),
        (
            "attempt-1/retry-attempt-evaluation.json",
            "a2d.senior-swe-bench-retry-attempt-evaluation.v1",
        ),
        (
            "attempt-1/retry-attempt-step-execution.json",
            "a2d.senior-swe-bench-retry-attempt-step-execution.v1",
        ),
        (
            "attempt-1/retry-attempt-step-evidence-execution.json",
            "a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1",
        ),
        (
            "attempt-1/retry-run-result.json",
            "a2d.senior-swe-bench-retry-run-result.v1",
        ),
        (
            "retry-resume-attempt-execution.json",
            "a2d.senior-swe-bench-retry-execution.v1",
        ),
    ] {
        let artifact: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.work_dir.join(relative)).unwrap_or_else(|error| {
                panic!(
                    "missing {}: {error}",
                    fixture.work_dir.join(relative).display()
                )
            }),
        )
        .unwrap();
        assert_eq!(artifact["schema_version"].as_str(), Some(schema));
    }

    let persisted: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.work_dir.join("retry-resume-attempt-execution.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted, value);
    assert_eq!(
        fs::read(
            fixture
                .work_dir
                .join("attempt-0/retry-attempt-evaluation.json")
        )
        .unwrap(),
        attempt0_evaluation_before
    );
    assert!(
        value["attempts"][1]["local_evaluation_path"]
            .as_str()
            .unwrap()
            .contains("attempt-1/local-evaluation.json")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_builds_next_cycle_input_without_fitness_claim() {
    let fixture = write_fixture(
        "resume-attempt-execute-next-input",
        3,
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
    assert_eq!(plan.status.code(), Some(0));

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"].as_str(), Some("failed"));
    assert_eq!(
        value["stop_reason"].as_str(),
        Some("precomputed_attempt_manifests_exhausted")
    );
    assert_eq!(value["attempts_executed"].as_u64(), Some(2));
    assert_eq!(
        value["fitness_claim_allowed_after_evidence_inspection"].as_bool(),
        Some(false)
    );
    assert!(value.get("terminal_run_result").is_none());
    let next_cycle_input_path = fixture.work_dir.join("attempt-1/next-cycle-input.json");
    let next_cycle_output_dir = fixture.work_dir.join("attempt-2/cycle-output-artifacts");
    assert!(next_cycle_input_path.is_file());
    assert_eq!(
        value["next_cycle_command"]["argv"],
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
        value["next_cycle_command"]["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert!(
        fixture
            .work_dir
            .join("retry-resume-attempt-execution.json")
            .is_file()
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_tampered_output_path_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-tampered-output",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));
    let mut plan_value: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let original_output = fixture.work_dir.join("attempt-1/local-evaluation.json");
    fs::write(&original_output, b"existing local evaluation\n").unwrap();
    plan_value["planned_outputs"]["local_evaluation"] = serde_json::json!(
        fixture
            .work_dir
            .join("attempt-1/fresh-local-evaluation.json")
            .to_string_lossy()
    );
    let tampered = serde_json::to_vec(&plan_value).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&tampered)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("planned local_evaluation does not match evaluate_args --output")
    );
    assert_eq!(
        fs::read(&original_output).unwrap(),
        b"existing local evaluation\n"
    );
    assert!(!fixture.root.join("evaluator-ran").exists());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_final_symlink_output_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-final-symlink-output",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));

    let outside_target = fixture.root.join("outside-local-evaluation.json");
    std::os::unix::fs::symlink(
        &outside_target,
        fixture.work_dir.join("attempt-1/local-evaluation.json"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not be a symlink"));
    assert!(!outside_target.exists());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_symlink_output_escape_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-symlink-output",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));

    let outside = fixture.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let escape = fixture.work_dir.join("attempt-1/escape");
    std::os::unix::fs::symlink(&outside, &escape).unwrap();
    let escaped_output = escape.join("local-evaluation.json");
    let mut plan_value: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    plan_value["planned_outputs"]["local_evaluation"] =
        serde_json::json!(escaped_output.to_string_lossy());
    let evaluate_args = plan_value["evaluate_args"].as_array_mut().unwrap();
    let output_index = evaluate_args
        .iter()
        .position(|arg| arg.as_str() == Some("--output"))
        .unwrap()
        + 1;
    evaluate_args[output_index] = serde_json::json!(escaped_output.to_string_lossy());
    let retry_step_args = plan_value["retry_step_args"].as_array_mut().unwrap();
    let local_evaluation_index = retry_step_args
        .iter()
        .position(|arg| arg.as_str() == Some("--local-evaluation"))
        .unwrap()
        + 1;
    retry_step_args[local_evaluation_index] = serde_json::json!(escaped_output.to_string_lossy());
    let tampered = serde_json::to_vec(&plan_value).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&tampered)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("must be under attempt dir"));
    assert!(!outside.join("local-evaluation.json").exists());
    assert!(!fixture.root.join("evaluator-ran").exists());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_stale_manifest_after_planning_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-stale-manifest",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));

    let replacement_manifest = write_manifest(
        &next_manifest_dir,
        "replacement",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+other\n",
    );
    fs::rename(&replacement_manifest, &next_manifest).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest hash"));
    assert!(!fixture.root.join("evaluator-ran").exists());
    assert!(
        !fixture
            .work_dir
            .join("attempt-1/local-evaluation.json")
            .exists()
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_mismatched_retry_plan_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-mismatched-retry-plan",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));
    fs::write(&fixture.retry_plan, sample_retry_plan(1)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("max_attempts"));
    assert!(!fixture.root.join("evaluator-ran").exists());
    assert!(
        !fixture
            .work_dir
            .join("attempt-1/local-evaluation.json")
            .exists()
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_resume_attempt_execute_rejects_existing_resumed_outputs_before_evaluator() {
    let fixture = write_fixture(
        "resume-execute-existing-output",
        2,
        "echo evaluator should not run >> ../evaluator-ran\nexit 1\n",
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
    let plan = Command::new(env!("CARGO_BIN_EXE_a2d"))
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
        .expect("run retry resume attempt plan");
    assert_eq!(plan.status.code(), Some(0));
    let existing = fixture.work_dir.join("attempt-1/local-evaluation.json");
    fs::write(&existing, b"stale local evaluation\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-retry-resume-attempt-execute", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("stdin").write_all(&plan.stdout)?;
            child.wait_with_output()
        })
        .expect("run retry resume attempt execute");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("planned output already exists"));
    assert_eq!(fs::read(&existing).unwrap(), b"stale local evaluation\n");
    assert!(!fixture.root.join("evaluator-ran").exists());

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
