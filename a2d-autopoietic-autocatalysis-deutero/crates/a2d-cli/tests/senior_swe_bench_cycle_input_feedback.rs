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

#[test]
fn senior_swe_bench_cycle_input_feedback_emits_coder_visible_feedback() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-cycle-input-feedback-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    let evaluation = root.join("local-evaluation.json");
    fs::write(
        &cycle_input,
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
"#,
    )
    .unwrap();
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
            "senior-swe-bench-cycle-input-feedback",
            cycle_input.to_str().unwrap(),
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run feedback command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );
    assert!(value["evaluation"]["fitness"].is_null());
    assert!(value.get("fitness_report").is_none());
    assert!(value.get("failure_report").is_none());
    let design = value["design"].as_str().unwrap();
    assert!(design.contains("SENIOR SWE-BENCH EVALUATOR FEEDBACK"));
    assert!(design.contains("status: failed"));
    assert!(design.contains("candidate_patch_hash: abc12300"));
    assert!(design.contains("missing route assertion"));
    assert!(design.contains("not a seeded fitness_report or failure_report"));
    assert_eq!(
        value["benchmark_context"]["github_solution_search_allowed"].as_bool(),
        Some(false)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn senior_swe_bench_cycle_input_feedback_rejects_malformed_inputs_before_cycle() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-cycle-input-feedback-bad-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    let evaluation = root.join("local-evaluation.json");
    fs::write(&cycle_input, r#"{"not":"a senior swe bench cycle input"}"#).unwrap();
    fs::write(&evaluation, r#"{"schema_version":"wrong"}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-cycle-input-feedback",
            cycle_input.to_str().unwrap(),
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("run feedback command");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("A²D Catalytic Cycle"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Senior SWE-Bench cycle input feedback error"),
        "{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}
