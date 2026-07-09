use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn git_hash_object_file(path: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["hash-object", path.to_str().unwrap()])
        .output()
        .expect("hash file");
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn write_executable_script(path: &std::path::Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn swe_bench_pro_readiness_blocks_without_reviewed_pro_manifest() {
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .arg("swe-bench-pro-readiness")
        .output()
        .expect("run SWE-Bench Pro readiness gate");

    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.swe-bench-pro-readiness.v1")
    );
    assert_eq!(value["status"].as_str(), Some("blocked"));
    assert_eq!(
        value["blocker"].as_str(),
        Some("missing_reviewed_swe_bench_pro_access_artifact")
    );
    assert_eq!(value["can_start_a2d_iteration"].as_bool(), Some(false));
    assert_eq!(value["benchmark_sources_loaded"].as_bool(), Some(false));
    assert_eq!(value["solution_material_loaded"].as_bool(), Some(false));
    assert_eq!(
        value["senior_swe_bench_manifest_accepted_as_pro"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
}

#[test]
fn swe_bench_pro_readiness_rejects_senior_swe_bench_manifest_as_not_pro() {
    let root =
        std::env::temp_dir().join(format!("a2d-swe-bench-pro-readiness-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let manifest = root.join("senior-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
            "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_provided_command": ["./official-evaluator"]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-readiness",
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run SWE-Bench Pro readiness gate");

    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"].as_str(), Some("blocked"));
    assert_eq!(
        value["blocker"].as_str(),
        Some("senior_swe_bench_manifest_is_not_swe_bench_pro")
    );
    assert_eq!(
        value["manifest_path"].as_str(),
        Some(manifest.to_string_lossy().as_ref())
    );
    assert_eq!(
        value["senior_swe_bench_manifest_accepted_as_pro"].as_bool(),
        Some(false)
    );
    assert_eq!(value["can_start_a2d_iteration"].as_bool(), Some(false));
    assert_eq!(value["benchmark_sources_loaded"].as_bool(), Some(false));
    assert_eq!(value["solution_material_loaded"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_access_manifest_inspect_validates_blind_non_leaking_manifest_without_evaluator() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-access-manifest-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let public_context = root.join("public-context.json");
    fs::write(
        &public_context,
        serde_json::to_vec_pretty(&serde_json::json!({
            "instance_id": "swe-pro-001",
            "repo": "owner/repo",
            "problem_statement": "Public task context only. No hidden tests or solution material."
        }))
        .unwrap(),
    )
    .unwrap();
    let public_context_hash = git_hash_object_file(&public_context);
    let evaluator = root.join("sealed-evaluator.sh");
    let sentinel = root.join("evaluator-ran");
    write_executable_script(&evaluator, &format!("touch {}", sentinel.display()));
    let manifest = root.join("pro-access-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.swe-bench-pro-access-manifest.v1",
            "benchmark": "swe-bench-pro",
            "instance_id": "swe-pro-001",
            "repo": "owner/repo",
            "public_context_path": public_context,
            "public_context_hash": public_context_hash,
            "sealed_evaluator_command": [evaluator],
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_sources_visible_to_a2d": false,
            "solution_material_visible_to_a2d": false,
            "evaluator_output_policy": "pass_fail_metrics_only"
        }))
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-access-manifest-inspect",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("inspect Pro access manifest");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.swe-bench-pro-access-manifest-inspection.v1")
    );
    assert_eq!(value["benchmark"].as_str(), Some("swe-bench-pro"));
    assert_eq!(value["instance_id"].as_str(), Some("swe-pro-001"));
    assert_eq!(value["manifest_valid"].as_bool(), Some(true));
    assert_eq!(
        value["evaluator_invocations_started"].as_bool(),
        Some(false)
    );
    assert_eq!(value["benchmark_sources_loaded"].as_bool(), Some(false));
    assert_eq!(value["solution_material_loaded"].as_bool(), Some(false));
    assert_eq!(
        value["coder_visible_context_kind"].as_str(),
        Some("public_context_only")
    );
    assert_eq!(
        value["evaluator_output_policy"].as_str(),
        Some("pass_fail_metrics_only")
    );
    assert_eq!(
        value["sealed_evaluator_command_redacted"].as_bool(),
        Some(true)
    );
    assert!(value["sealed_evaluator_command_hash"].as_str().is_some());
    assert!(value.get("sealed_evaluator_command").is_none());
    assert_eq!(value["manifest_path_redacted"].as_bool(), Some(true));
    assert_eq!(value["public_context_path_redacted"].as_bool(), Some(true));
    assert!(value.get("manifest_path").is_none());
    assert!(value.get("public_context_path").is_none());
    let inspection_stdout = String::from_utf8_lossy(&output.stdout);
    let inspection_stderr = String::from_utf8_lossy(&output.stderr);
    for hidden_path in [&evaluator, &manifest, &public_context] {
        assert!(
            !inspection_stdout.contains(hidden_path.to_string_lossy().as_ref()),
            "inspection stdout must not expose access-manifest paths: {inspection_stdout}"
        );
        assert!(
            !inspection_stderr.contains(hidden_path.to_string_lossy().as_ref()),
            "inspection stderr must not expose access-manifest paths: {inspection_stderr}"
        );
    }
    assert_eq!(
        value["fitness_claim_allowed_before_evidence"].as_bool(),
        Some(false)
    );
    assert!(
        !sentinel.exists(),
        "manifest inspection must not run sealed evaluator"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_access_manifest_inspect_accepts_stdin_manifest_without_path_leak() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-access-manifest-stdin-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let public_context = root.join("public-context.json");
    fs::write(&public_context, b"{}\n").unwrap();
    let public_context_hash = git_hash_object_file(&public_context);
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 0");
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version": "a2d.swe-bench-pro-access-manifest.v1",
        "benchmark": "swe-bench-pro",
        "instance_id": "swe-pro-001",
        "repo": "owner/repo",
        "public_context_path": public_context,
        "public_context_hash": public_context_hash,
        "sealed_evaluator_command": [evaluator],
        "hidden_holdouts": true,
        "github_solution_search_allowed": false,
        "benchmark_sources_visible_to_a2d": false,
        "solution_material_visible_to_a2d": false,
        "evaluator_output_policy": "pass_fail_metrics_only"
    }))
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["swe-bench-pro-access-manifest-inspect", "--manifest", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn manifest inspect");
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), &manifest).unwrap();
    let output = child
        .wait_with_output()
        .expect("inspect Pro stdin manifest");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["manifest_valid"].as_bool(), Some(true));
    assert_eq!(value["manifest_path_redacted"].as_bool(), Some(true));
    assert!(value["manifest_hash"].as_str().is_some());
    assert!(value.get("manifest_path").is_none());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for hidden_path in [&public_context, &evaluator] {
        assert!(
            !stdout.contains(hidden_path.to_string_lossy().as_ref()),
            "{stdout}"
        );
        assert!(
            !stderr.contains(hidden_path.to_string_lossy().as_ref()),
            "{stderr}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_access_manifest_inspect_rejects_source_solution_or_secret_paths() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-access-manifest-bad-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let public_context = root.join("public-context.json");
    fs::write(&public_context, b"{}\n").unwrap();
    let public_context_hash = git_hash_object_file(&public_context);
    let manifest = root.join("bad-pro-access-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.swe-bench-pro-access-manifest.v1",
            "benchmark": "swe-bench-pro",
            "instance_id": "swe-pro-001",
            "repo": "owner/repo",
            "public_context_path": public_context,
            "public_context_hash": public_context_hash,
            "sealed_evaluator_command": ["./sealed-evaluator"],
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_sources_visible_to_a2d": false,
            "solution_material_visible_to_a2d": false,
            "evaluator_output_policy": "pass_fail_metrics_only",
            "solution_patch_path": "secret/solution.diff"
        }))
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-access-manifest-inspect",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("inspect Pro access manifest");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("forbidden benchmark-private field"),
        "{stderr}"
    );
    assert!(!stderr.contains("solution_patch_path"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("solution.diff"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_access_manifest_inspect_rejects_unknown_fields_and_redacts_paths() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-access-manifest-unknown-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let public_context = root.join("public-context.json");
    fs::write(&public_context, b"{}\n").unwrap();
    let public_context_hash = git_hash_object_file(&public_context);
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 0");
    let manifest = root.join("unknown-pro-access-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.swe-bench-pro-access-manifest.v1",
            "benchmark": "swe-bench-pro",
            "instance_id": "swe-pro-001",
            "repo": "owner/repo",
            "public_context_path": public_context,
            "public_context_hash": public_context_hash,
            "sealed_evaluator_command": [evaluator],
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_sources_visible_to_a2d": false,
            "solution_material_visible_to_a2d": false,
            "evaluator_output_policy": "pass_fail_metrics_only",
            "notes": "would-be private source or hidden evaluator topology"
        }))
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-access-manifest-inspect",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("inspect Pro access manifest");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("unknown field in SWE-Bench Pro access manifest"),
        "{stderr}"
    );
    for hidden_text in [
        manifest.to_string_lossy().to_string(),
        public_context.to_string_lossy().to_string(),
        evaluator.to_string_lossy().to_string(),
        "would-be private".to_string(),
        "hidden evaluator topology".to_string(),
    ] {
        assert!(
            !stderr.contains(&hidden_text),
            "stderr leaked {hidden_text}: {stderr}"
        );
        assert!(
            !stdout.contains(&hidden_text),
            "stdout leaked {hidden_text}: {stdout}"
        );
    }

    let _ = fs::remove_dir_all(root);
}
