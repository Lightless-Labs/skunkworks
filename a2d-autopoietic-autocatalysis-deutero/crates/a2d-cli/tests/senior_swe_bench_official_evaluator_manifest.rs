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

fn sample_cycle_input() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
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
    }))
    .unwrap()
}

fn git_hash_object_file(path: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["hash-object", path.to_str().unwrap()])
        .output()
        .expect("git hash-object");
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
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

fn sample_manifest(command: &[&str]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
        "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
        "task_id": "task-hard",
        "repo": "owner/repo",
        "hidden_holdouts": true,
        "github_solution_search_allowed": false,
        "benchmark_provided_command": command
    }))
    .unwrap()
}

#[test]
fn official_evaluator_manifest_inspect_validates_without_running_evaluator() {
    let root =
        std::env::temp_dir().join(format!("a2d-official-manifest-inspect-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    let manifest = root.join("official-manifest.json");
    let sentinel = root.join("evaluator-ran");
    let evaluator = root.join("official-evaluator.sh");
    write_executable_script(&evaluator, &format!("touch {}", sentinel.to_string_lossy()));
    let evaluator_text = evaluator.to_string_lossy().to_string();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(&manifest, sample_manifest(&[&evaluator_text, "official"])).unwrap();
    let expected_manifest_hash = git_hash_object_file(&manifest);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-official-evaluator-manifest-inspect",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
            "--",
            evaluator.to_str().unwrap(),
            "official",
        ])
        .output()
        .expect("run official manifest inspect");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1")
    );
    assert_eq!(value["task_id"].as_str(), Some("task-hard"));
    assert_eq!(value["repo"].as_str(), Some("owner/repo"));
    assert_eq!(
        value["official_evaluator_manifest_path"].as_str(),
        Some(manifest.to_string_lossy().as_ref())
    );
    assert_eq!(
        value["official_evaluator_manifest_hash"].as_str(),
        Some(expected_manifest_hash.as_str())
    );
    assert_eq!(
        value["official_benchmark_provided_command"],
        serde_json::json!([evaluator.to_string_lossy(), "official"])
    );
    assert_eq!(value["official_hidden_holdouts"].as_bool(), Some(true));
    assert_eq!(
        value["official_github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["evaluator_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_evidence_inspection_started"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["official_senior_swe_bench_mastery"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert!(
        !sentinel.exists(),
        "manifest inspection must not run the evaluator"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn official_evaluator_manifest_inspect_rejects_command_mismatch_before_evaluator() {
    let root = std::env::temp_dir().join(format!(
        "a2d-official-manifest-inspect-mismatch-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let cycle_input = root.join("cycle-input.json");
    let manifest = root.join("official-manifest.json");
    let sentinel = root.join("evaluator-ran");
    let evaluator = root.join("official-evaluator.sh");
    write_executable_script(&evaluator, &format!("touch {}", sentinel.to_string_lossy()));
    let evaluator_text = evaluator.to_string_lossy().to_string();
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    fs::write(&manifest, sample_manifest(&[&evaluator_text, "official"])).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-official-evaluator-manifest-inspect",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
            "--",
            evaluator.to_str().unwrap(),
            "tampered",
        ])
        .output()
        .expect("run official manifest inspect");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("benchmark_provided_command"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !sentinel.exists(),
        "manifest mismatch must fail before evaluator execution"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn official_evaluator_manifest_inspect_is_listed_in_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .arg("definitely-not-a-command")
        .output()
        .expect("run invalid command");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("senior-swe-bench-official-evaluator-manifest-inspect"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
