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
    assert_eq!(value["manifest_path_redacted"].as_bool(), Some(true));
    assert!(value.get("manifest_path").is_none());
    assert!(value.get("benchmark_url").is_none());
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

fn write_pro_manifest(
    root: &std::path::Path,
    evaluator: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let public_context = root.join("public-context.json");
    fs::write(
        &public_context,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "a2d.swe-bench-pro-public-context.v1",
            "instance_id": "swe-pro-001",
            "repo": "owner/repo",
            "problem_statement": "Public synthetic task context only.",
            "allowed_context_summary": ["Use the supplied checkout and local tests only."],
            "forbidden_context": ["hidden tests", "reference solutions", "public solution search"]
        }))
        .unwrap(),
    )
    .unwrap();
    let public_context_hash = git_hash_object_file(&public_context);
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
    (manifest, public_context)
}

fn write_tiny_checkout_and_patch(
    root: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let checkout = root.join("checkout");
    fs::create_dir_all(&checkout).unwrap();
    fs::write(checkout.join("lib.rs"), "original\n").unwrap();
    let patch = root.join("candidate.patch");
    fs::write(
        &patch,
        "diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-original\n+patched\n",
    )
    .unwrap();
    (checkout, patch)
}

#[test]
fn swe_bench_pro_readiness_accepts_valid_reviewed_pro_manifest_without_running_evaluator() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-ready-manifest-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let sentinel = root.join("evaluator-ran");
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, &format!("touch {}", sentinel.display()));
    let (manifest, public_context) = write_pro_manifest(&root, &evaluator);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-readiness",
            "--official-evaluator-manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run ready SWE-Bench Pro gate");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"].as_str(), Some("ready"));
    assert!(value["blocker"].is_null());
    assert_eq!(value["can_start_a2d_iteration"].as_bool(), Some(true));
    assert_eq!(value["instance_id"].as_str(), Some("swe-pro-001"));
    assert_eq!(value["repo"].as_str(), Some("owner/repo"));
    assert_eq!(value["manifest_path_redacted"].as_bool(), Some(true));
    assert_eq!(value["public_context_path_redacted"].as_bool(), Some(true));
    assert_eq!(
        value["sealed_evaluator_command_redacted"].as_bool(),
        Some(true)
    );
    assert!(value.get("manifest_path").is_none());
    assert!(value.get("public_context_path").is_none());
    assert!(value.get("sealed_evaluator_command").is_none());
    assert!(!sentinel.exists(), "readiness must not execute evaluator");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for hidden_path in [&manifest, &public_context, &evaluator] {
        assert!(!stdout.contains(hidden_path.to_string_lossy().as_ref()));
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_public_context_and_redacted_failure_bridge_two_attempts_without_leaks() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-cycle-bridge-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 1");
    let (manifest, public_context) = write_pro_manifest(&root, &evaluator);
    let (_checkout, patch) = write_tiny_checkout_and_patch(&root);

    let first = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-cycle-input",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("build first Pro cycle input");
    assert_eq!(
        first.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_value: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        first_value["benchmark_context"]["schema_version"].as_str(),
        Some("a2d.swe-bench-pro-cycle-context.v1")
    );
    assert_eq!(first_value["evaluation"]["attempt"].as_u64(), Some(0));
    assert_eq!(
        first_value["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );
    assert!(
        first_value["requirements"]
            .as_str()
            .unwrap()
            .contains("Return only a unified diff")
            || first_value["requirements"]
                .as_str()
                .unwrap()
                .contains("return only a unified diff")
    );
    assert_eq!(
        first_value["benchmark_context"]["github_solution_search_allowed"].as_bool(),
        Some(false)
    );
    assert!(first_value.get("sealed_evaluator_command").is_none());
    assert!(first_value.get("public_context_path").is_none());

    let first_path = root.join("cycle-input-0.json");
    fs::write(&first_path, &first.stdout).unwrap();
    let canary = "EVALUATOR_PRIVATE_CANARY_DO_NOT_COPY";
    let context = &first_value["benchmark_context"];
    let failed_evaluation = serde_json::json!({
        "schema_version": "a2d.swe-bench-pro-sealed-evaluation.v1",
        "benchmark": "swe-bench-pro",
        "instance_id": context["instance_id"],
        "repo": context["repo"],
        "manifest_hash": context["manifest_hash"],
        "manifest_path_redacted": true,
        "public_context_hash": context["public_context_hash"],
        "public_context_path_redacted": true,
        "sealed_evaluator_command_hash": context["sealed_evaluator_command_hash"],
        "sealed_evaluator_command_redacted": true,
        "evaluator_kind": "sealed_swe_bench_pro",
        "evaluator_output_policy": "pass_fail_metrics_only",
        "candidate_patch_hash": git_hash_object_file(&patch),
        "candidate_patch_path_redacted": true,
        "candidate_patch_preflight_checked": true,
        "candidate_patch_preflight_status": "passed",
        "candidate_patch_preflight_command_redacted": true,
        "candidate_patch_applied": true,
        "evaluator_checkout_mode": "isolated_copy",
        "evaluator_checkout_redacted": true,
        "original_checkout_mutated": false,
        "status": "failed",
        "exit_code": 1,
        "hidden_holdouts": true,
        "github_solution_search_allowed": false,
        "benchmark_sources_loaded": false,
        "solution_material_loaded": false,
        "benchmark_sources_visible_to_a2d": false,
        "solution_material_visible_to_a2d": false,
        "evaluator_stdout_redacted": true,
        "evaluator_stderr_redacted": true,
        "fitness_evidence_exported": false,
        "fitness_evidence_path_redacted": false,
        "fitness_evidence_hash": null,
        "fitness_claim_allowed": false,
        "note": canary
    });
    let evaluation_path = root.join("sealed-evaluation.json");
    fs::write(
        &evaluation_path,
        serde_json::to_vec_pretty(&failed_evaluation).unwrap(),
    )
    .unwrap();

    let second = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-cycle-input-feedback",
            first_path.to_str().unwrap(),
            evaluation_path.to_str().unwrap(),
        ])
        .output()
        .expect("build second Pro cycle input");
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_text = String::from_utf8(second.stdout).unwrap();
    let second_value: serde_json::Value = serde_json::from_str(&second_text).unwrap();
    assert_eq!(second_value["evaluation"]["attempt"].as_u64(), Some(1));
    assert_eq!(
        second_value["evaluation"]["status"].as_str(),
        Some("not_evaluated")
    );
    assert!(second_value["evaluation"]["fitness"].is_null());
    assert!(
        second_value["design"]
            .as_str()
            .unwrap()
            .contains("status: failed")
    );
    assert!(
        second_value["plan"]
            .as_str()
            .unwrap()
            .contains("revised unified diff")
    );
    for hidden in [
        canary.to_string(),
        manifest.to_string_lossy().to_string(),
        public_context.to_string_lossy().to_string(),
        evaluator.to_string_lossy().to_string(),
        evaluation_path.to_string_lossy().to_string(),
    ] {
        assert!(
            !second_text.contains(&hidden),
            "retry input leaked {hidden}"
        );
    }

    let mut bad_evaluation = failed_evaluation;
    bad_evaluation
        .as_object_mut()
        .unwrap()
        .insert("evaluator_stdout".to_string(), serde_json::json!(canary));
    fs::write(
        &evaluation_path,
        serde_json::to_vec_pretty(&bad_evaluation).unwrap(),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-cycle-input-feedback",
            first_path.to_str().unwrap(),
            evaluation_path.to_str().unwrap(),
        ])
        .output()
        .expect("reject non-redacted Pro feedback");
    assert_eq!(rejected.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&rejected.stderr).contains(canary));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_cycle_input_rejects_unknown_public_context_fields_without_echoing_values() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-public-context-reject-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 1");
    let (manifest, public_context) = write_pro_manifest(&root, &evaluator);
    let private_canary = "PRIVATE_SOLUTION_CANARY";
    let mut context: serde_json::Value =
        serde_json::from_slice(&fs::read(&public_context).unwrap()).unwrap();
    context.as_object_mut().unwrap().insert(
        "solution_patch".to_string(),
        serde_json::json!(private_canary),
    );
    fs::write(
        &public_context,
        serde_json::to_vec_pretty(&context).unwrap(),
    )
    .unwrap();
    let mut manifest_value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    manifest_value["public_context_hash"] =
        serde_json::Value::String(git_hash_object_file(&public_context));
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&manifest_value).unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-cycle-input",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("reject private Pro public-context field");
    assert_eq!(output.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(private_canary));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(private_canary));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_evaluate_runs_sealed_evaluator_without_persisting_hidden_output_or_paths() {
    let root = std::env::temp_dir().join(format!("a2d-swe-bench-pro-evaluate-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let sentinel = root.join("evaluator-ran");
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(
        &evaluator,
        &format!(
            "set -eu\ntest \"$(cat lib.rs)\" = \"patched\"\ntest \"$A2D_SWE_BENCH_PRO_CANDIDATE_PATCH_APPLIED\" = \"true\"\ntest \"$A2D_SWE_BENCH_PRO_EVALUATOR_CHECKOUT_MODE\" = \"isolated_copy\"\ntest \"$A2D_SWE_BENCH_PRO_GITHUB_SOLUTION_SEARCH_ALLOWED\" = \"false\"\ntest \"$A2D_SWE_BENCH_PRO_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\" = \"true\"\ntest -z \"${{HTTP_PROXY:-}}\"\ntouch {}\necho hidden-private-output\necho hidden-private-error >&2",
            sentinel.display()
        ),
    );
    let (manifest, public_context) = write_pro_manifest(&root, &evaluator);
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);
    let output_path = root.join("evaluation.json");
    let fitness_dir = root.join("fitness");
    let expected_patch_hash = git_hash_object_file(&patch);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-evaluate",
            "--manifest",
            manifest.to_str().unwrap(),
            "--candidate-patch",
            patch.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--apply-candidate-patch",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .env("HTTP_PROXY", "http://forbidden.invalid")
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", &fitness_dir)
        .output()
        .expect("run sealed Pro evaluator");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        sentinel.exists(),
        "sealed evaluator should run for evaluate command"
    );
    assert_eq!(
        fs::read_to_string(checkout.join("lib.rs")).unwrap(),
        "original\n"
    );
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.swe-bench-pro-sealed-evaluation.v1")
    );
    assert_eq!(value["status"].as_str(), Some("passed"));
    assert_eq!(
        value["evaluator_kind"].as_str(),
        Some("sealed_swe_bench_pro")
    );
    assert_eq!(
        value["candidate_patch_hash"].as_str(),
        Some(expected_patch_hash.as_str())
    );
    assert_eq!(
        value["sealed_evaluator_command_redacted"].as_bool(),
        Some(true)
    );
    assert_eq!(value["evaluator_stdout_redacted"].as_bool(), Some(true));
    assert_eq!(value["evaluator_stderr_redacted"].as_bool(), Some(true));
    assert_eq!(value["fitness_evidence_exported"].as_bool(), Some(true));
    assert_eq!(
        value["fitness_evidence_path_redacted"].as_bool(),
        Some(true)
    );
    assert!(value["fitness_evidence_hash"].as_str().is_some());
    assert!(value.get("sealed_evaluator_command").is_none());
    assert!(value.get("evaluator_stdout").is_none());
    assert!(value.get("evaluator_stderr").is_none());
    assert!(value.get("fitness_evidence_path").is_none());
    let evidence_files: Vec<_> = fs::read_dir(&fitness_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(evidence_files.len(), 1, "{evidence_files:?}");
    let evidence_text = fs::read_to_string(&evidence_files[0]).unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();
    assert_eq!(
        evidence["schema_version"].as_str(),
        Some("a2d.fitness-evidence.v1")
    );
    assert_eq!(
        evidence["evaluator_kind"].as_str(),
        Some("sealed_swe_bench_pro")
    );
    assert_eq!(
        evidence["candidate_patch_hash"].as_str(),
        Some(expected_patch_hash.as_str())
    );
    let evidence_command = evidence["evidence_command"].as_str().unwrap();
    assert!(evidence_command.contains("swe-bench-pro-evaluate"));
    assert!(evidence_command.contains("--manifest <redacted>"));
    assert!(evidence_command.contains("--candidate-patch <redacted>"));
    assert!(evidence_command.contains("--checkout <redacted>"));
    assert!(evidence_command.contains("--output <redacted>"));
    assert!(evidence.get("candidate_patch_path").is_none());
    assert!(evidence.get("candidate_patch_artifact_path").is_none());
    assert!(evidence.get("evaluator_checkout").is_none());
    assert!(evidence.get("candidate_patch_preflight_command").is_none());
    assert!(
        evidence
            .get("official_benchmark_provided_command")
            .is_none()
    );
    for (field, value) in [
        ("candidate_patch_path", serde_json::json!(patch)),
        ("candidate_patch_artifact_path", serde_json::json!(patch)),
        ("evaluator_checkout", serde_json::json!(checkout)),
        (
            "candidate_patch_preflight_command",
            serde_json::json!("git apply --check private.patch"),
        ),
        (
            "official_benchmark_provided_command",
            serde_json::json!(["sealed-evaluator.sh"]),
        ),
    ] {
        let mut bad = evidence.clone();
        bad.as_object_mut()
            .unwrap()
            .insert(field.to_string(), value);
        let bad_path = root.join(format!("bad-pro-evidence-{field}.json"));
        fs::write(&bad_path, serde_json::to_vec_pretty(&bad).unwrap()).unwrap();
        let inspected = Command::new(env!("CARGO_BIN_EXE_a2d"))
            .args([
                "fitness-evidence-inspect",
                bad_path.to_str().unwrap(),
                "--require-all-tests-pass",
            ])
            .output()
            .expect("inspect bad Pro evidence");
        assert!(
            !inspected.status.success(),
            "fitness-evidence-inspect accepted {field}: stdout={} stderr={}",
            String::from_utf8_lossy(&inspected.stdout),
            String::from_utf8_lossy(&inspected.stderr)
        );
    }
    let persisted = fs::read_to_string(&output_path).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for hidden in [
        "hidden-private-output".to_string(),
        "hidden-private-error".to_string(),
        evaluator.to_string_lossy().to_string(),
        manifest.to_string_lossy().to_string(),
        public_context.to_string_lossy().to_string(),
        patch.to_string_lossy().to_string(),
        checkout.to_string_lossy().to_string(),
    ] {
        assert!(
            !persisted.contains(&hidden),
            "persisted output leaked {hidden}: {persisted}"
        );
        assert!(
            !stdout.contains(&hidden),
            "stdout leaked {hidden}: {stdout}"
        );
        assert!(
            !stderr.contains(&hidden),
            "stderr leaked {hidden}: {stderr}"
        );
        assert!(
            !evidence_text.contains(&hidden),
            "fitness evidence leaked {hidden}: {evidence_text}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_evaluate_accepts_only_aggregate_metrics_from_completed_failure() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-evaluate-metrics-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(
        &evaluator,
        "printf '%s\\n' '{\"schema_version\":\"a2d.swe-bench-pro-sealed-result.v1\",\"passed\":false,\"metrics\":{\"passed\":7,\"failed\":2,\"total\":9}}'; exit 1",
    );
    let (manifest, _) = write_pro_manifest(&root, &evaluator);
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);
    let output_path = root.join("evaluation.json");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-evaluate",
            "--manifest",
            manifest.to_str().unwrap(),
            "--candidate-patch",
            patch.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--apply-candidate-patch",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run failed Pro evaluator with aggregate metrics");

    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(value["status"].as_str(), Some("failed"));
    assert_eq!(value["exit_code"].as_i64(), Some(1));
    assert_eq!(value["aggregate_metrics"]["passed"].as_u64(), Some(7));
    assert_eq!(value["aggregate_metrics"]["failed"].as_u64(), Some(2));
    assert_eq!(value["aggregate_metrics"]["total"].as_u64(), Some(9));
    assert!(value.get("evaluator_stdout").is_none());
    assert!(value.get("evaluator_stderr").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_evaluate_classifies_non_contract_exit_as_infrastructure_error() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-evaluate-infra-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 3");
    let (manifest, _) = write_pro_manifest(&root, &evaluator);
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);
    let output_path = root.join("evaluation.json");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-evaluate",
            "--manifest",
            manifest.to_str().unwrap(),
            "--candidate-patch",
            patch.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--apply-candidate-patch",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("run infrastructure-failing Pro evaluator");

    assert_eq!(output.status.code(), Some(3));
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(value["status"].as_str(), Some("infrastructure_error"));
    assert_eq!(value["exit_code"].as_i64(), Some(3));
    assert!(value["aggregate_metrics"].is_null());
    assert_eq!(value["fitness_claim_allowed"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_evaluate_rejects_private_manifest_fields_before_evaluator() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-evaluate-bad-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let sentinel = root.join("evaluator-ran");
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, &format!("touch {}", sentinel.display()));
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
            "sealed_evaluator_command": [evaluator],
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_sources_visible_to_a2d": false,
            "solution_material_visible_to_a2d": false,
            "evaluator_output_policy": "pass_fail_metrics_only",
            "hidden_tests_path": "private/hidden-tests"
        }))
        .unwrap(),
    )
    .unwrap();
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-evaluate",
            "--manifest",
            manifest.to_str().unwrap(),
            "--candidate-patch",
            patch.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
        ])
        .output()
        .expect("run sealed Pro evaluator");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        !sentinel.exists(),
        "private manifest must fail before evaluator execution"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("forbidden benchmark-private field"),
        "{stderr}"
    );
    assert!(!stderr.contains("hidden_tests_path"), "{stderr}");
    assert!(!stdout.contains("private/hidden-tests"), "{stdout}");

    let _ = fs::remove_dir_all(root);
}

fn add_executable_hash_to_manifest(manifest: &std::path::Path, evaluator: &std::path::Path) {
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
    value["sealed_evaluator_executable_hash"] =
        serde_json::Value::String(git_hash_object_file(evaluator));
    fs::write(manifest, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn run_pro_pressure_cycle_input(
    cycle_input: &std::path::Path,
    evaluation: &std::path::Path,
    manifest: &std::path::Path,
    patch: &std::path::Path,
    checkout: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "cycle-input",
            cycle_input.to_str().unwrap(),
            "1",
            "--swe-bench-pro-evaluation",
            evaluation.to_str().unwrap(),
            "--swe-bench-pro-manifest",
            manifest.to_str().unwrap(),
            "--swe-bench-pro-candidate-patch",
            patch.to_str().unwrap(),
            "--swe-bench-pro-evaluator-checkout",
            checkout.to_str().unwrap(),
        ])
        .output()
        .expect("run cycle-input in Pro pressure mode")
}

fn write_pro_cycle_input(root: &std::path::Path, manifest: &std::path::Path) -> std::path::PathBuf {
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-cycle-input",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("build Pro cycle input");
    assert!(output.status.success(), "{output:?}");
    let path = root.join("cycle-input.json");
    fs::write(&path, output.stdout).unwrap();
    path
}

#[test]
fn swe_bench_pro_pressure_mode_requires_pinned_evaluator_before_any_cycle() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-pressure-unpinned-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(&evaluator, "exit 1");
    let (manifest, _) = write_pro_manifest(&root, &evaluator);
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);
    let cycle_input = write_pro_cycle_input(&root, &manifest);
    let evaluation = root.join("evaluation.json");
    fs::write(&evaluation, "{}").unwrap();

    let output =
        run_pro_pressure_cycle_input(&cycle_input, &evaluation, &manifest, &patch, &checkout);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires sealed_evaluator_executable_hash"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Catalytic Cycle"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swe_bench_pro_pressure_mode_rejects_tampered_recorded_evaluation_before_any_cycle() {
    let root = std::env::temp_dir().join(format!(
        "a2d-swe-bench-pro-pressure-tampered-{}",
        unique_suffix()
    ));
    fs::create_dir_all(&root).unwrap();
    let evaluator = root.join("sealed-evaluator.sh");
    write_executable_script(
        &evaluator,
        "printf '%s\\n' '{\"schema_version\":\"a2d.swe-bench-pro-sealed-result.v1\",\"passed\":false,\"metrics\":{\"passed\":7,\"failed\":2,\"total\":9}}'; exit 1",
    );
    let (manifest, _) = write_pro_manifest(&root, &evaluator);
    add_executable_hash_to_manifest(&manifest, &evaluator);
    let (checkout, patch) = write_tiny_checkout_and_patch(&root);
    let cycle_input = write_pro_cycle_input(&root, &manifest);
    let evaluation = root.join("evaluation.json");
    let recorded = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "swe-bench-pro-evaluate",
            "--manifest",
            manifest.to_str().unwrap(),
            "--candidate-patch",
            patch.to_str().unwrap(),
            "--checkout",
            checkout.to_str().unwrap(),
            "--apply-candidate-patch",
            "--output",
            evaluation.to_str().unwrap(),
        ])
        .output()
        .expect("record sealed Pro evaluation");
    assert_eq!(recorded.status.code(), Some(2), "{recorded:?}");

    // Claim a better score than the sealed evaluator actually reported.
    let mut tampered: serde_json::Value =
        serde_json::from_slice(&fs::read(&evaluation).unwrap()).unwrap();
    tampered["aggregate_metrics"] = serde_json::json!({"passed": 8, "failed": 1, "total": 9});
    fs::write(&evaluation, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();

    let output =
        run_pro_pressure_cycle_input(&cycle_input, &evaluation, &manifest, &patch, &checkout);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match trusted fresh re-evaluation"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Catalytic Cycle"));
    assert_eq!(
        fs::read_to_string(checkout.join("lib.rs")).unwrap(),
        "original\n"
    );

    let _ = fs::remove_dir_all(root);
}
