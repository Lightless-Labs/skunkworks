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
        .expect("git hash-object stdin")
        .write_all(bytes)
        .expect("write git hash-object stdin");
    let output = child.wait_with_output().expect("git hash-object output");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("a2d-cli is under project crates directory")
        .to_path_buf()
}

fn git_output_at(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn repo_relative_crates_scope(root: &Path) -> String {
    format!(
        "{}crates",
        git_output_at(root, &["rev-parse", "--show-prefix"])
    )
}

fn current_crates_source_revision(root: &Path) -> String {
    let scope = repo_relative_crates_scope(root);
    git_output_at(root, &["rev-parse", &format!("HEAD:{scope}")])
}

fn current_crates_status(root: &Path) -> String {
    let scope = repo_relative_crates_scope(root);
    git_output_at(
        root,
        &["status", "--short", "--", &format!(":(top){scope}")],
    )
}

fn current_crates_diff_hash(root: &Path) -> String {
    let scope = repo_relative_crates_scope(root);
    let output = Command::new("git")
        .args(["diff", "--binary", "HEAD", "--", &format!(":(top){scope}")])
        .current_dir(root)
        .output()
        .expect("git diff crates");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    git_hash_object_bytes(&output.stdout)
}

fn project_relative(path: &Path) -> String {
    path.strip_prefix(project_root())
        .expect("path under project root")
        .to_string_lossy()
        .replace('\\', "/")
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
fn fitness_evidence_inspect_resolves_official_repo_relative_paths_from_non_root_cwd() {
    let project_root = project_root();
    let fixture_relative = PathBuf::from("target").join(format!(
        "a2d-official-evidence-cli-nonroot-{}",
        unique_suffix()
    ));
    let fixture_root = project_root.join(&fixture_relative);
    fs::create_dir_all(&fixture_root).unwrap();
    let manifest = fixture_root.join("official-manifest.json");
    let inspection = fixture_root.join("official-manifest-inspection.json");
    let cycle_input = fixture_root.join("cycle-input.json");
    let evaluator = fixture_root.join("official-evaluator.sh");
    let non_root_cwd = std::env::temp_dir().join(format!(
        "a2d-official-evidence-nonroot-cwd-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&non_root_cwd).unwrap();
    write_executable_script(&evaluator, "exit 0");
    fs::write(&cycle_input, sample_cycle_input()).unwrap();
    let evaluator_text = evaluator.to_string_lossy().to_string();
    fs::write(&manifest, sample_manifest(&[&evaluator_text])).unwrap();

    let inspection_output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-official-evaluator-manifest-inspect",
            "--task-cycle-input",
            cycle_input.to_str().unwrap(),
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
            "--",
            evaluator.to_str().unwrap(),
        ])
        .output()
        .expect("run official manifest inspect");
    assert_eq!(
        inspection_output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&inspection_output.stderr)
    );
    fs::write(&inspection, inspection_output.stdout).unwrap();

    let evidence_path = fixture_root.join("official-fitness-evidence.json");
    fs::write(
        &evidence_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.fitness-evidence.v1",
            "cycle": 0,
            "passed": 4,
            "failed": 0,
            "total": 4,
            "fitness": 1.0,
            "delta_from_last_non_regressing_fitness": 0.0,
            "non_regressing": true,
            "actual_tests_evaluated": true,
            "diagnostic_present": false,
            "failed_cases": [],
            "results": [
                {"name": "compiles", "passed": true},
                {"name": "has_tests", "passed": true},
                {"name": "all_tests_pass", "passed": true},
                {"name": "hidden_acceptance", "passed": true}
            ],
            "source_revision": current_crates_source_revision(&project_root),
            "source_tree_dirty": !current_crates_status(&project_root).is_empty(),
            "source_diff_scope": "crates",
            "source_diff_hash": current_crates_diff_hash(&project_root),
            "evidence_command": "integration-test official evidence path resolution",
            "evaluator_kind": "official_senior_swe_bench",
            "official_evaluator_manifest_path": project_relative(&manifest),
            "official_evaluator_manifest_hash": git_hash_object_file(&manifest),
            "official_evaluator_manifest_inspection_path": project_relative(&inspection),
            "official_evaluator_manifest_inspection_hash": git_hash_object_file(&inspection),
            "official_evaluator_manifest_inspection_validated": true,
            "official_benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
            "official_task_id": "task-hard",
            "official_repo": "owner/repo",
            "official_hidden_holdouts": true,
            "official_github_solution_search_allowed": false,
            "official_benchmark_provided_command": [evaluator_text]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(&non_root_cwd)
        .args([
            "fitness-evidence-inspect",
            evidence_path.to_str().unwrap(),
            "--require-all-tests-pass",
        ])
        .output()
        .expect("inspect official fitness evidence from non-root cwd");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let outside_manifest = non_root_cwd.join("outside-official-manifest.json");
    fs::write(&outside_manifest, fs::read(&manifest).unwrap()).unwrap();
    let mut outside_evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(&evidence_path).unwrap()).unwrap();
    outside_evidence["official_evaluator_manifest_path"] =
        serde_json::json!(outside_manifest.to_string_lossy().to_string());
    fs::write(
        &evidence_path,
        serde_json::to_vec_pretty(&outside_evidence).unwrap(),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .current_dir(&non_root_cwd)
        .args([
            "fitness-evidence-inspect",
            evidence_path.to_str().unwrap(),
            "--require-all-tests-pass",
        ])
        .output()
        .expect("inspect official fitness evidence with outside absolute manifest path");
    assert_eq!(rejected.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("resolves outside the A²D project root"),
        "stderr={}",
        String::from_utf8_lossy(&rejected.stderr)
    );

    let _ = fs::remove_dir_all(fixture_root);
    let _ = fs::remove_dir_all(non_root_cwd);
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
