use a2d_providers::cli::network_configuration_env_vars;
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

fn evaluator_network_env_absence_shell() -> String {
    network_configuration_env_vars()
        .into_iter()
        .map(|key| format!("test -z \"${{{key}-}}\" || {{ echo '{key} leaked' >&2; exit 42; }}\n"))
        .collect()
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

struct Fixture {
    root: std::path::PathBuf,
    extraction: std::path::PathBuf,
    patch: std::path::PathBuf,
    local_evaluation: std::path::PathBuf,
    marker: std::path::PathBuf,
}

fn project_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate lives under project crates directory")
        .to_path_buf()
}

fn write_fixture(name: &str, evaluator_body: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "a2d-retry-attempt-evaluate-{name}-{}",
        unique_suffix()
    ));
    write_fixture_at(root, evaluator_body)
}

fn write_fixture_at(root: std::path::PathBuf, evaluator_body: &str) -> Fixture {
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
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    let artifact = root.join("candidate.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&artifact, diff).unwrap();
    let patch = attempt.join("candidate.patch");
    fs::write(&patch, diff).unwrap();
    let local_evaluation = attempt.join("local-evaluation.json");
    let marker = root.join("evaluator-ran.marker");
    let evaluator = root.join("evaluate.sh");
    fs::write(
        &evaluator,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ntouch '{}'\n{}\n",
            marker.display(),
            evaluator_body
        ),
    )
    .unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(&evaluator)
        .status()
        .expect("chmod evaluator");

    let extraction = root.join("extraction.json");
    fs::write(
        &extraction,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-retry-attempt-extraction.v1",
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
                "--", evaluator
            ],
            "retry_step_args": [
                "senior-swe-bench-retry-step",
                "--retry-plan", root.join("retry-plan.json"),
                "--attempt-index", "0",
                "--task-cycle-input", cycle_input,
                "--local-evaluation", local_evaluation
            ],
            "provider_invocations_started": false,
            "evaluator_invocations_started": false,
            "fitness_claim_allowed_before_evidence": false,
            "github_solution_search_allowed": false
        }))
        .unwrap(),
    )
    .unwrap();

    Fixture {
        root,
        extraction,
        patch,
        local_evaluation,
        marker,
    }
}

#[test]
fn retry_attempt_evaluate_runs_planned_evaluator_once_and_emits_next_args() {
    let fixture = write_fixture(
        "passed",
        "test \"${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}\" = true\ngrep -q new src/lib.rs\n",
    );
    let evidence_dir = fixture.root.join("fitness");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &evidence_dir)
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.marker.exists(), "evaluator should run exactly once");
    assert!(fixture.local_evaluation.exists());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-retry-attempt-evaluation.v1")
    );
    assert_eq!(value["local_evaluation_status"].as_str(), Some("passed"));
    assert_eq!(value["evaluator_invocations_started"].as_bool(), Some(true));
    assert_eq!(value["retry_step_started"].as_bool(), Some(false));
    assert_eq!(
        value["fitness_evidence_inspection_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["retry_step_args"].as_array().unwrap()[0].as_str(),
        Some("senior-swe-bench-retry-step")
    );
    assert!(value["fitness_evidence_path"].as_str().is_some());

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_scrubs_network_env_while_preserving_no_search_policy_env() {
    let fixture = write_fixture(
        "env-scrub",
        &format!(
            "test \"${{A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED}}\" = false\n\
             test \"${{A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}}\" = true\n{}\
             grep -q new src/lib.rs\n",
            evaluator_network_env_absence_shell()
        ),
    );
    let evidence_dir = fixture.root.join("fitness");
    let mut command = Command::new(env!("CARGO_BIN_EXE_a2d"));
    command
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &evidence_dir)
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ]);
    for key in network_configuration_env_vars() {
        command.env(key, "http://example.invalid:9");
    }

    let output = command.output().expect("run retry attempt evaluate");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.marker.exists(), "evaluator should run exactly once");
    let local_evaluation: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.local_evaluation).unwrap()).unwrap();
    assert_eq!(local_evaluation["status"].as_str(), Some("passed"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_serializes_in_project_fitness_evidence_path_repo_relative() {
    let root = project_root().join("target").join(format!(
        "a2d-retry-attempt-evaluate-repo-relative-{}",
        unique_suffix()
    ));
    let fixture = write_fixture_at(
        root,
        "test \"${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}\" = true\ngrep -q new src/lib.rs\n",
    );
    let evidence_dir = fixture.root.join("fitness");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &evidence_dir)
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let fitness_path = value["fitness_evidence_path"].as_str().unwrap();
    assert!(
        !fitness_path.starts_with('/'),
        "in-project fitness evidence path should be repo-relative: {fitness_path}"
    );
    assert!(
        fitness_path.starts_with("target/a2d-retry-attempt-evaluate-repo-relative-"),
        "unexpected repo-relative fitness evidence path: {fitness_path}"
    );
    assert!(project_root().join(fitness_path).is_file());

    let local_evaluation_text = fs::read_to_string(&fixture.local_evaluation).unwrap();
    assert!(
        !local_evaluation_text.contains(project_root().to_str().unwrap()),
        "in-project local evaluation should not leak project-root host paths: {local_evaluation_text}"
    );
    assert!(
        !local_evaluation_text.contains(std::env::temp_dir().to_str().unwrap()),
        "isolated evaluator temp checkout should be represented by a portable marker: {local_evaluation_text}"
    );
    let local_evaluation: serde_json::Value = serde_json::from_str(&local_evaluation_text).unwrap();
    assert_eq!(
        local_evaluation["fitness_evidence_path"].as_str(),
        Some(fitness_path)
    );
    assert_eq!(
        local_evaluation["evaluator_checkout"].as_str(),
        Some("isolated_temp_checkout")
    );
    assert!(
        local_evaluation["candidate_patch_preflight_command"]
            .as_str()
            .unwrap()
            .contains("target/a2d-retry-attempt-evaluate-repo-relative-"),
        "preflight command should use a repo-relative candidate patch path: {local_evaluation_text}"
    );

    let evidence_text = fs::read_to_string(project_root().join(fitness_path)).unwrap();
    assert!(
        !evidence_text.contains(project_root().to_str().unwrap()),
        "in-project fitness evidence should not leak project-root host paths: {evidence_text}"
    );
    assert!(
        !evidence_text.contains(std::env::temp_dir().to_str().unwrap()),
        "isolated evaluator temp checkout should not leak into fitness evidence: {evidence_text}"
    );
    let evidence: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();
    assert_eq!(
        evidence["evaluator_checkout"].as_str(),
        Some("isolated_temp_checkout")
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_serializes_external_relative_fitness_evidence_path_as_absolute() {
    let fixture = write_fixture(
        "external-relative-fitness",
        "test \"${A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN}\" = true\ngrep -q new src/lib.rs\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(&fixture.root)
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", "fitness")
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate from external cwd");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let fitness_path = value["fitness_evidence_path"].as_str().unwrap();
    let fitness_path = std::path::Path::new(fitness_path);
    assert!(
        fitness_path.is_absolute(),
        "external relative export dir should serialize as an absolute CWD-stable path: {}",
        fitness_path.display()
    );
    let canonical_fitness_path = fs::canonicalize(fitness_path).unwrap();
    let canonical_expected_dir = fs::canonicalize(fixture.root.join("fitness")).unwrap();
    assert!(
        canonical_fitness_path.starts_with(&canonical_expected_dir),
        "fitness evidence should resolve under the external retry work dir {}, got {}",
        canonical_expected_dir.display(),
        canonical_fitness_path.display()
    );
    assert!(canonical_fitness_path.is_file());

    let local_evaluation_text = fs::read_to_string(&fixture.local_evaluation).unwrap();
    assert!(
        !local_evaluation_text.contains(project_root().to_str().unwrap()),
        "external retry artifacts should not leak the project-root host path: {local_evaluation_text}"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_allows_evaluator_separator_arguments() {
    let fixture = write_fixture(
        "evaluator-separator-arg",
        "test \"$#\" -eq 2\ntest \"$1\" = --\ntest \"$2\" = ignored-sentinel\ngrep -q new src/lib.rs\n",
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.extraction).unwrap()).unwrap();
    let args = value["evaluate_args"].as_array_mut().unwrap();
    args.push(serde_json::json!("--"));
    args.push(serde_json::json!("ignored-sentinel"));
    fs::write(
        &fixture.extraction,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["local_evaluation_status"].as_str(), Some("passed"));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_preserves_failed_evaluation_for_retry_step() {
    let fixture = write_fixture("failed", "echo public failure >&2\nexit 1\n");
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.marker.exists());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["local_evaluation_status"].as_str(), Some("failed"));
    assert!(value.get("fitness_evidence_path").is_none());
    assert_eq!(value["retry_step_started"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_fails_closed_before_evaluator_on_patch_tamper() {
    let fixture = write_fixture("tampered", "echo should-not-run\n");
    fs::write(&fixture.patch, b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+evil\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("candidate patch hash mismatch"));
    assert!(
        !fixture.marker.exists(),
        "evaluator must not run after tamper"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_rejects_duplicate_evaluator_flags_before_running() {
    let fixture = write_fixture("duplicate-output", "echo should-not-run\n");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.extraction).unwrap()).unwrap();
    let args = value["evaluate_args"].as_array_mut().unwrap();
    let separator = args
        .iter()
        .position(|arg| arg.as_str() == Some("--"))
        .unwrap();
    args.insert(
        separator,
        serde_json::json!(fixture.root.join("other-local-evaluation.json")),
    );
    args.insert(separator, serde_json::json!("--output"));
    fs::write(
        &fixture.extraction,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate --output"));
    assert!(
        !fixture.marker.exists(),
        "evaluator must not run with ambiguous duplicate flags"
    );

    let _ = fs::remove_dir_all(fixture.root);
}

#[test]
fn retry_attempt_evaluate_rejects_retry_step_local_evaluation_mismatch() {
    let fixture = write_fixture("retry-step-mismatch", "echo should-not-run\n");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.extraction).unwrap()).unwrap();
    let args = value["retry_step_args"].as_array_mut().unwrap();
    let local_evaluation_value = args
        .iter_mut()
        .skip_while(|arg| arg.as_str() != Some("--local-evaluation"))
        .nth(1)
        .expect("local evaluation arg value");
    *local_evaluation_value = serde_json::json!(fixture.root.join("other-local-evaluation.json"));
    fs::write(
        &fixture.extraction,
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-attempt-evaluate",
            fixture.extraction.to_str().unwrap(),
        ])
        .output()
        .expect("run retry attempt evaluate");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("local-evaluation does not match"));
    assert!(
        !fixture.marker.exists(),
        "evaluator must not run when next retry-step would use a different evaluation"
    );

    let _ = fs::remove_dir_all(fixture.root);
}
