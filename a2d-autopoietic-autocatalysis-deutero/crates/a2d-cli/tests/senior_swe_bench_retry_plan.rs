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

#[test]
fn senior_swe_bench_retry_plan_is_bounded_and_evidence_gated() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-retry-plan-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-plan",
            cycle_input.to_str().unwrap(),
            "2",
        ])
        .output()
        .expect("run retry plan command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-cycle-retry-plan.v1")
    );
    assert_eq!(value["max_attempts"].as_u64(), Some(2));
    assert_eq!(value["provider_invocations_started"].as_bool(), Some(false));
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["information_barriers"]["public_github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert_eq!(value["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["attempts"][0]["on_evaluation_failed"].as_str(),
        Some("build_next_cycle_input_with_senior_swe_bench_cycle_input_feedback")
    );
    assert_eq!(
        value["attempts"][1]["on_evaluation_failed"].as_str(),
        Some("stop_attempts_exhausted_without_fitness_claim")
    );
    let success_requires = value["success_requires"].as_array().unwrap();
    assert!(
        success_requires
            .iter()
            .any(|entry| entry.as_str() == Some("a2d.fitness-evidence.v1"))
    );
    assert!(
        success_requires
            .iter()
            .any(|entry| entry.as_str() == Some("non_regressing:true"))
    );
    assert!(
        success_requires
            .iter()
            .any(|entry| entry.as_str() == Some("actual_tests_evaluated:true"))
    );
    let root_entries = fs::read_dir(&root)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        root_entries.len(),
        1,
        "retry-plan is a stdout-only planning artifact and must not create hidden durable files"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn senior_swe_bench_retry_plan_rejects_unbounded_or_unsafe_inputs() {
    let root = std::env::temp_dir().join(format!(
        "a2d-senior-swe-bench-retry-plan-bad-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();

    let zero = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-plan",
            cycle_input.to_str().unwrap(),
            "0",
        ])
        .output()
        .expect("run retry plan command");
    assert_eq!(zero.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&zero.stderr).contains("max_attempts"));

    let too_many = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-plan",
            cycle_input.to_str().unwrap(),
            "9",
        ])
        .output()
        .expect("run retry plan command");
    assert_eq!(too_many.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&too_many.stderr).contains("<= 8"));

    let unsafe_cycle_input = root.join("unsafe-cycle-input.json");
    fs::write(
        &unsafe_cycle_input,
        sample_cycle_input().replace(
            "\"github_solution_search_allowed\": false",
            "\"github_solution_search_allowed\": true",
        ),
    )
    .unwrap();
    let unsafe_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-retry-plan",
            unsafe_cycle_input.to_str().unwrap(),
            "2",
        ])
        .output()
        .expect("run retry plan command");
    assert_eq!(unsafe_output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unsafe_output.stderr).contains("solution search"));

    let _ = fs::remove_dir_all(root);
}
