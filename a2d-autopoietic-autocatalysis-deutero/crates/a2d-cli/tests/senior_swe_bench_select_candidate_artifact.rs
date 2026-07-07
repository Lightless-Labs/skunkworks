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

fn write_manifest(root: &std::path::Path, artifacts: serde_json::Value) -> std::path::PathBuf {
    let manifest = root.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "a2d.cycle-output-artifacts.v1",
            "artifacts": artifacts,
        }))
        .unwrap(),
    )
    .unwrap();
    manifest
}

#[test]
fn senior_swe_bench_select_candidate_artifact_selects_exact_coder_output() {
    let root = std::env::temp_dir().join(format!("a2d-select-candidate-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let artifact = root.join("cycle-0-wc-0001-coder-code.artifact");
    let diff = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fs::write(&artifact, diff).unwrap();
    let hash = git_hash_object_bytes(diff);
    let manifest = write_manifest(
        &root,
        serde_json::json!([
            {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": hash,
                "bytes": diff.len()
            }
        ]),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-select-candidate-artifact",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run select command");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["schema_version"].as_str(),
        Some("a2d.senior-swe-bench-candidate-artifact-selection.v1")
    );
    assert_eq!(value["selected"]["enzyme_id"].as_str(), Some("coder"));
    assert_eq!(value["selected"]["artifact_type"].as_str(), Some("code"));
    assert_eq!(
        value["selected"]["git_object_hash"].as_str(),
        Some(hash.as_str())
    );
    assert_eq!(
        value["contains_unified_diff_candidate_patch"].as_bool(),
        Some(true)
    );
    assert_eq!(
        value["failure_kind"].as_str(),
        Some("candidate_patch_extractable")
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
    assert_eq!(
        value["extract_patch_args"].as_array().unwrap()[0].as_str(),
        Some("senior-swe-bench-extract-patch")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn senior_swe_bench_select_candidate_artifact_fails_closed_on_unsafe_manifests() {
    let root = std::env::temp_dir().join(format!("a2d-select-candidate-bad-{}", unique_suffix()));
    fs::create_dir_all(&root).unwrap();
    let artifact = root.join("candidate.artifact");
    let prose = b"I'll inspect the local checkout first.";
    fs::write(&artifact, prose).unwrap();
    let hash = git_hash_object_bytes(prose);
    let manifest = write_manifest(
        &root,
        serde_json::json!([
            {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": hash,
                "bytes": prose.len()
            }
        ]),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-select-candidate-artifact",
            manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run select command");
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["contains_unified_diff_candidate_patch"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["failure_kind"].as_str(),
        Some("checkout_context_not_exercised")
    );

    let bad_hash_manifest = root.join("bad-hash-manifest.json");
    fs::write(
        &bad_hash_manifest,
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
                "git_object_hash": "0000000000000000000000000000000000000000",
                "bytes": prose.len()
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-select-candidate-artifact",
            bad_hash_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run select command");
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("hash mismatch"));

    let second_artifact = root.join("candidate-2.artifact");
    fs::write(&second_artifact, prose).unwrap();
    let multi_manifest = write_manifest(
        &root,
        serde_json::json!([
            {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": artifact,
                "git_object_hash": hash,
                "bytes": prose.len()
            },
            {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0002",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": second_artifact,
                "git_object_hash": hash,
                "bytes": prose.len()
            }
        ]),
    );
    let rejected_multi = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-select-candidate-artifact",
            multi_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run select command");
    assert_eq!(rejected_multi.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected_multi.stderr).contains("exactly one"));

    let safe_artifact = root.join("safe-gh-pr-fragments.artifact");
    let safe_diff = b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nNotes: high priority fix through PR review for this GH project; gh_pr_number metadata is local.\n";
    fs::write(&safe_artifact, safe_diff).unwrap();
    let safe_manifest = write_manifest(
        &root,
        serde_json::json!([
            {
                "cycle": 0,
                "report_cycle": 1,
                "workcell_id": "wc-0001",
                "enzyme_id": "coder",
                "provider": "test-provider",
                "artifact_type": "code",
                "path": safe_artifact,
                "git_object_hash": git_hash_object_bytes(safe_diff),
                "bytes": safe_diff.len()
            }
        ]),
    );
    let accepted_safe = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "senior-swe-bench-select-candidate-artifact",
            safe_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("run select command");
    assert_eq!(
        accepted_safe.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&accepted_safe.stderr)
    );
    let safe_value: serde_json::Value = serde_json::from_slice(&accepted_safe.stdout).unwrap();
    assert_eq!(
        safe_value["contains_public_github_solution_reference"].as_bool(),
        Some(false)
    );
    assert_eq!(
        safe_value["failure_kind"].as_str(),
        Some("candidate_patch_extractable")
    );

    for (name, public) in [
        (
            "public-url",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nhttps://github.com/example/repo/pull/1\n".as_slice(),
        ),
        (
            "public-git-remote",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nfrom git@github.com:example/repo.git\n".as_slice(),
        ),
        (
            "public-ref",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nrefs/pull/123/head\n".as_slice(),
        ),
        (
            "public-raw-content",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nhttps://raw.githubusercontent.com/example/repo/main/fix.diff\n".as_slice(),
        ),
        (
            "public-obfuscated-url",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\ngithub[.]com/example/repo/issues/1\n".as_slice(),
        ),
        (
            "public-dot-word-url",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\ngithub dot com/example/repo/pull/1\n".as_slice(),
        ),
        (
            "public-spaced-dot-word-url",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\ngithub . com/example/repo/commit/deadbeef\n".as_slice(),
        ),
        (
            "public-percent-encoded-github-host",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: https://github%2ecom/example/repo/pull/1\n".as_slice(),
        ),
        (
            "public-percent-encoded-pull-ref",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: refs%2fpull%2f123%2fhead\n".as_slice(),
        ),
        (
            "public-gh-pr-command",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: gh pr view 123 --repo example/repo\n".as_slice(),
        ),
        (
            "public-gh-api-command",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: gh api repos/example/repo/pulls/123/files\n".as_slice(),
        ),
        (
            "public-hub-pr-command",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: hub pr checkout 123\n".as_slice(),
        ),
        (
            "public-hub-search-command",
            b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nsource: hub search pulls example/repo\n".as_slice(),
        ),
    ] {
        let public_artifact = root.join(format!("{name}.artifact"));
        fs::write(&public_artifact, public).unwrap();
        let public_manifest = write_manifest(
            &root,
            serde_json::json!([
                {
                    "cycle": 0,
                    "report_cycle": 1,
                    "workcell_id": "wc-0001",
                    "enzyme_id": "coder",
                    "provider": "test-provider",
                    "artifact_type": "code",
                    "path": public_artifact,
                    "git_object_hash": git_hash_object_bytes(public),
                    "bytes": public.len()
                }
            ]),
        );
        let rejected_public = Command::new(env!("CARGO_BIN_EXE_a2d"))
            .args([
                "senior-swe-bench-select-candidate-artifact",
                public_manifest.to_str().unwrap(),
            ])
            .output()
            .expect("run select command");
        assert_eq!(rejected_public.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&rejected_public.stderr).contains("public GitHub"));
    }

    let _ = fs::remove_dir_all(root);
}
