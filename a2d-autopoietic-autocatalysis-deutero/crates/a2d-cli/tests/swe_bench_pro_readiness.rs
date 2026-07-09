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
