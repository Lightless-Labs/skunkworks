use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn diagnose_artifact_classifies_checkout_deferral_without_claiming_fitness() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-diagnose-artifact", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn diagnose command");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"I'll inspect the local checkout and identify the issue.")
        .expect("write artifact");

    let output = child.wait_with_output().expect("wait for diagnose command");

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).trim().is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnosis is JSON");
    assert_eq!(
        value
            .get("schema_version")
            .and_then(serde_json::Value::as_str),
        Some("a2d.senior-swe-bench-artifact-diagnosis.v1")
    );
    assert_eq!(
        value
            .get("failure_kind")
            .and_then(serde_json::Value::as_str),
        Some("checkout_context_not_exercised")
    );
    assert_eq!(
        value
            .get("contains_unified_diff_candidate_patch")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(
        value
            .get("note")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .contains("not fitness evidence")
    );
}

#[test]
fn diagnose_artifact_classifies_valid_diff_as_candidate_patch_only() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-diagnose-artifact", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn diagnose command");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nNotes: issue id data %2541 and deep issue data %25252525252525252541 are local metadata, not public sources.\n")
        .expect("write artifact");

    let output = child.wait_with_output().expect("wait for diagnose command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnosis is JSON");
    assert_eq!(
        value
            .get("failure_kind")
            .and_then(serde_json::Value::as_str),
        Some("candidate_patch_extractable")
    );
    assert_eq!(
        value
            .get("contains_unified_diff_candidate_patch")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        value
            .get("contains_public_github_solution_reference")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(
        value
            .get("recommended_next_gate")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .contains("senior-swe-bench-evaluate")
    );
}

#[test]
fn diagnose_artifact_redacts_public_github_reference_without_diff() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-diagnose-artifact", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn diagnose command");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"The answer is at https://github.com/org/repo/commit/deadbeef")
        .expect("write artifact");

    let output = child.wait_with_output().expect("wait for diagnose command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnosis is JSON");
    assert_eq!(
        value
            .get("contains_public_github_solution_reference")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        value
            .get("failure_kind")
            .and_then(serde_json::Value::as_str),
        Some("public_solution_reference")
    );
    assert_eq!(
        value
            .get("contains_unified_diff_candidate_patch")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    let preview = value
        .get("artifact_preview")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(preview.contains("redacted"), "{preview}");
    assert!(!preview.contains("deadbeef"), "{preview}");
    assert!(
        !preview.to_ascii_lowercase().contains("github.com"),
        "{preview}"
    );
}

#[test]
fn diagnose_artifact_does_not_flag_non_command_gh_pr_fragments() {
    let artifact = b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\nNotes: high priority fix through PR review for this GH project; gh_pr_number metadata is local.\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["senior-swe-bench-diagnose-artifact", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn diagnose command");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(artifact)
        .expect("write artifact");

    let output = child.wait_with_output().expect("wait for diagnose command");

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnosis is JSON");
    assert_eq!(
        value
            .get("contains_public_github_solution_reference")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        value
            .get("failure_kind")
            .and_then(serde_json::Value::as_str),
        Some("candidate_patch_extractable")
    );
}

#[test]
fn diagnose_artifact_redacts_mixed_case_public_github_references() {
    for artifact in [
        b"Copied from HTTPS://GitHub.com/org/repo/PuLl/1\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n".as_slice(),
        b"Patch came from git@github.com:org/repo.git".as_slice(),
        b"Patch came from refs/pull/123/head".as_slice(),
        b"Patch copied from https://raw.githubusercontent.com/org/repo/main/fix.diff".as_slice(),
        b"Patch copied from github[.]com/org/repo/issues/123".as_slice(),
        b"Patch copied from github dot com/org/repo/pull/123".as_slice(),
        b"Patch copied from github . com/org/repo/commit/deadbeef".as_slice(),
        b"Patch copied from https://gist.github.com/org/abcdef123456".as_slice(),
        b"Patch copied from https://github%2ecom/org/repo/pull/123".as_slice(),
        b"Patch copied from https://github%252ecom/org/repo/pull/123".as_slice(),
        b"Patch copied from https://github%25252ecom/org/repo/pull/123".as_slice(),
        b"Patch copied from https://github%2525252525252525252ecom/org/repo/pull/123".as_slice(),
        b"Patch copied from https%253a%252f%252f%2547%2569%2574%2548%2575%2562%252e%2563%256f%256d%252forg%252frepo%252fpull%252f123".as_slice(),
        b"Patch copied from refs%2fpull%2f123%2fhead".as_slice(),
        b"Patch copied from refs%252fpull%252f123%252fhead".as_slice(),
        b"Patch copied from refs%25252fpull%25252f123%25252fhead".as_slice(),
        b"Patch copied from refs%2525252525252525252fpull%2525252525252525252f123%2525252525252525252fhead".as_slice(),
        b"Use gh pr view 123 --repo org/repo to inspect the fix".as_slice(),
        b"Run gh api repos/org/repo/pulls/123/files for the patch".as_slice(),
        b"hub pr checkout 123 has the answer".as_slice(),
        b"hub search pulls org/repo has the answer".as_slice(),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
            .args(["senior-swe-bench-diagnose-artifact", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn diagnose command");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(artifact)
            .expect("write artifact");

        let output = child.wait_with_output().expect("wait for diagnose command");

        assert_eq!(output.status.code(), Some(0));
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("diagnosis is JSON");
        assert_eq!(
            value
                .get("contains_public_github_solution_reference")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            value
                .get("failure_kind")
                .and_then(serde_json::Value::as_str),
            Some("public_solution_reference")
        );
        assert_eq!(
            value
                .get("contains_unified_diff_candidate_patch")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        let preview = value
            .get("artifact_preview")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(preview.contains("redacted"), "{preview}");
        assert!(
            !preview.to_ascii_lowercase().contains("github.com"),
            "{preview}"
        );
        assert!(!preview.contains("refs/pull"), "{preview}");
    }
}
