//! Self-sandbox: validate proposed modifications to A²D's own source code.
//!
//! The system that writes chess engines can now write itself — safely.
//! Proposed patches are compiled and tested in isolation before acceptance.
//! Protected files (the "physics") cannot be modified by automated actors.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A proposed modification to a system source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPatch {
    /// Relative path from project root (e.g., "crates/a2d-core/src/metabolism.rs")
    pub file_path: String,
    /// Complete new content for the file.
    pub new_content: String,
}

/// Result of validating a system patch in the self-sandbox.
#[derive(Debug, Clone)]
pub struct SelfSandboxResult {
    /// Did the patch pass all gates?
    pub accepted: bool,
    /// Did the modified system compile?
    pub compiled: bool,
    /// Did all tests pass?
    pub tests_passed: bool,
    /// Compiler output (stderr)
    pub compile_output: String,
    /// Test runner output
    pub test_output: String,
    /// Why was the patch rejected (if applicable)?
    pub rejection_reason: Option<String>,
}

/// Files that cannot be modified by automated actors.
/// These are the "physics" of the system — evaluation, gating, closure detection.
/// Constitution Invariants 2 and 4 require these to remain under human control.
pub const PROTECTED_FILES: &[&str] = &[
    "crates/a2d-core/src/germline.rs",
    "crates/a2d-core/src/raf.rs",
    "crates/a2d-core/src/sandbox.rs",
    "crates/a2d-core/src/benchmark.rs",
    "crates/a2d-core/src/self_sandbox.rs",
    "CONSTITUTION.md",
];

/// Files automated actors are allowed to modify through the SystemPatch gate.
///
/// This is narrower than "every non-protected Rust file". The architect's job is
/// to improve the A²D mechanism: orchestration, provider integration, runtime
/// routing, CLI wiring, and challenge contracts. Incidental library/demo modules
/// such as prime/email are intentionally excluded so a self-modification cannot
/// pass self-sandbox by rewriting unrelated code.
pub const AUTOMATED_MODIFIABLE_FILES: &[&str] = &[
    "crates/a2d-core/src/challenges.rs",
    "crates/a2d-core/src/lineage.rs",
    "crates/a2d-core/src/metabolism.rs",
    "crates/a2d-core/src/observer.rs",
    "crates/a2d-core/src/provider.rs",
    "crates/a2d-core/src/types.rs",
    "crates/a2d-core/src/workcell.rs",
    "crates/a2d-core/tests/bootstrap.rs",
    "crates/a2d-cli/src/main.rs",
    "crates/a2d-cli/tests/score_artifact.rs",
    "crates/a2d-providers/src/claude.rs",
    "crates/a2d-providers/src/cli.rs",
    "crates/a2d-providers/src/lib.rs",
];

fn normalize_patch_path(file_path: &str) -> String {
    file_path.replace('\\', "/")
}

/// Check if a file path is protected from automated modification.
pub fn is_protected(file_path: &str) -> bool {
    let normalized = normalize_patch_path(file_path);
    PROTECTED_FILES
        .iter()
        .any(|&p| normalized == p || normalized.ends_with(p))
}

/// Check if a file path is eligible for automated SystemPatch modification.
pub fn is_automated_modifiable(file_path: &str) -> bool {
    let normalized = normalize_patch_path(file_path);
    AUTOMATED_MODIFIABLE_FILES
        .iter()
        .any(|&p| normalized == p || normalized.ends_with(p))
}

/// Validate a proposed system patch by compiling and testing in isolation.
///
/// Gates (in order):
/// 1. Protected file check (Constitution enforcement)
/// 2. Automated-modifiable eligibility check (mechanism files only)
/// 3. Target file must exist (no creating new files via patch)
/// 4. Copy source tree to temp dir, apply patch
/// 5. `cargo test` must pass on the modified source
pub fn validate_patch(project_root: &Path, patch: &SystemPatch) -> SelfSandboxResult {
    validate_patches(project_root, std::slice::from_ref(patch))
}

/// Validate proposed system patches atomically in one isolated copy.
///
/// All static gates run before copying the project. If they pass, every patch is
/// applied to one temp tree and a single `cargo test` validates the combined
/// state. Callers must not apply or queue any member unless this returns
/// `accepted: true`.
pub fn validate_patches(project_root: &Path, patches: &[SystemPatch]) -> SelfSandboxResult {
    if patches.is_empty() {
        return SelfSandboxResult {
            accepted: false,
            compiled: false,
            tests_passed: false,
            compile_output: String::new(),
            test_output: String::new(),
            rejection_reason: Some("SystemPatch batch is empty".to_string()),
        };
    }

    let mut seen_paths = HashSet::new();
    for patch in patches {
        let normalized = normalize_patch_path(&patch.file_path);
        if !seen_paths.insert(normalized) {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!(
                    "Duplicate SystemPatch target in batch: {}",
                    patch.file_path
                )),
            };
        }

        if is_protected(&patch.file_path) {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!(
                    "Protected file: {} cannot be modified by automated actors (Constitution)",
                    patch.file_path
                )),
            };
        }

        if !is_automated_modifiable(&patch.file_path) {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!(
                    "File is not eligible for automated modification: {}",
                    patch.file_path
                )),
            };
        }

        let target = project_root.join(&patch.file_path);
        if !target.exists() {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!(
                    "File does not exist: {} — patches modify existing files only",
                    patch.file_path
                )),
            };
        }
    }

    let temp_dir = match tempfile::Builder::new()
        .prefix("a2d-self-sandbox-")
        .tempdir()
    {
        Ok(dir) => dir,
        Err(e) => {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!("Failed to create temp dir: {e}")),
            };
        }
    };

    if let Err(e) = copy_source_tree(project_root, temp_dir.path()) {
        return SelfSandboxResult {
            accepted: false,
            compiled: false,
            tests_passed: false,
            compile_output: String::new(),
            test_output: String::new(),
            rejection_reason: Some(format!("Failed to copy source tree: {e}")),
        };
    }

    for patch in patches {
        let patched_file = temp_dir.path().join(&patch.file_path);
        if let Some(parent) = patched_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&patched_file, &patch.new_content) {
            return SelfSandboxResult {
                accepted: false,
                compiled: false,
                tests_passed: false,
                compile_output: String::new(),
                test_output: String::new(),
                rejection_reason: Some(format!("Failed to write patch {}: {e}", patch.file_path)),
            };
        }
    }

    let test_result = Command::new("cargo")
        .arg("test")
        .current_dir(temp_dir.path())
        .output();

    match test_result {
        Ok(output) => {
            let compile_output = String::from_utf8_lossy(&output.stderr).to_string();
            let test_output = String::from_utf8_lossy(&output.stdout).to_string();
            let tests_passed = output.status.success();
            let compiled = !compile_output.contains("error[E");

            SelfSandboxResult {
                accepted: tests_passed,
                compiled,
                tests_passed,
                compile_output,
                test_output,
                rejection_reason: if tests_passed {
                    None
                } else if !compiled {
                    Some("Patch breaks compilation".to_string())
                } else {
                    Some("cargo test failed on modified source".to_string())
                },
            }
        }
        Err(e) => SelfSandboxResult {
            accepted: false,
            compiled: false,
            tests_passed: false,
            compile_output: String::new(),
            test_output: String::new(),
            rejection_reason: Some(format!("Failed to run cargo test: {e}")),
        },
    }
}

/// Read the current content of all modifiable system files.
/// Returns (relative_path, content) pairs for the architect's context.
pub fn read_modifiable_files(project_root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let eligible: HashSet<&str> = AUTOMATED_MODIFIABLE_FILES.iter().copied().collect();

    if let Ok(entries) = walk_rs_files(&project_root.join("crates")) {
        for entry in entries {
            if let Ok(relative) = entry.strip_prefix(project_root) {
                let rel_str = relative.to_string_lossy().replace('\\', "/");
                if eligible.contains(rel_str.as_str()) && !is_protected(&rel_str) {
                    if let Ok(content) = fs::read_to_string(&entry) {
                        files.push((rel_str, content));
                    }
                }
            }
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Copy the minimal source tree needed for `cargo test`.
fn copy_source_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    for name in &["Cargo.toml", "Cargo.lock"] {
        let src_file = src.join(name);
        if src_file.exists() {
            fs::copy(&src_file, dst.join(name))?;
        }
    }

    let crates_src = src.join("crates");
    if crates_src.exists() {
        copy_dir_recursive(&crates_src, &dst.join("crates"))?;
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn walk_rs_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    if !dir.is_dir() {
        return Ok(results);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            results.extend(walk_rs_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            results.push(path);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_files_are_rejected() {
        assert!(is_protected("crates/a2d-core/src/germline.rs"));
        assert!(is_protected("crates/a2d-core/src/raf.rs"));
        assert!(is_protected("crates/a2d-core/src/sandbox.rs"));
        assert!(is_protected("crates/a2d-core/src/benchmark.rs"));
        assert!(is_protected("crates/a2d-core/src/self_sandbox.rs"));
        assert!(is_protected("CONSTITUTION.md"));
    }

    #[test]
    fn mechanism_files_are_automated_modifiable() {
        assert!(is_automated_modifiable("crates/a2d-core/src/metabolism.rs"));
        assert!(is_automated_modifiable("crates/a2d-core/src/types.rs"));
        assert!(is_automated_modifiable("crates/a2d-core/src/challenges.rs"));
        assert!(is_automated_modifiable(
            "crates/a2d-core/tests/bootstrap.rs"
        ));
        assert!(is_automated_modifiable("crates/a2d-cli/src/main.rs"));
        assert!(is_automated_modifiable(
            "crates/a2d-cli/tests/score_artifact.rs"
        ));
        assert!(is_automated_modifiable("crates/a2d-providers/src/cli.rs"));
    }

    #[test]
    fn incidental_domain_files_are_not_automated_modifiable() {
        assert!(!is_protected("crates/a2d-core/src/prime.rs"));
        assert!(!is_protected("crates/a2d-core/src/email.rs"));
        assert!(!is_automated_modifiable("crates/a2d-core/src/prime.rs"));
        assert!(!is_automated_modifiable("crates/a2d-core/src/email.rs"));
    }

    #[test]
    fn windows_paths_normalized() {
        assert!(is_protected("crates\\a2d-core\\src\\germline.rs"));
    }

    #[test]
    fn patch_to_protected_file_rejected_immediately() {
        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/germline.rs".to_string(),
            new_content: "// hacked".to_string(),
        };
        // Use a dummy path — should never reach filesystem check
        let result = validate_patch(Path::new("/nonexistent"), &patch);
        assert!(!result.accepted);
        assert!(
            result
                .rejection_reason
                .as_ref()
                .unwrap()
                .contains("Protected file")
        );
    }

    #[test]
    fn patch_to_nonexistent_file_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/metabolism.rs".to_string(),
            new_content: "fn foo() {}".to_string(),
        };
        let result = validate_patch(temp.path(), &patch);
        assert!(!result.accepted);
        assert!(
            result
                .rejection_reason
                .as_ref()
                .unwrap()
                .contains("does not exist")
        );
    }

    #[test]
    fn patch_to_ineligible_file_rejected_before_filesystem_check() {
        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/prime.rs".to_string(),
            new_content: "pub fn is_prime(_: i64) -> Result<bool, String> { Ok(true) }".to_string(),
        };
        let result = validate_patch(Path::new("/nonexistent"), &patch);
        assert!(!result.accepted);
        assert!(
            result
                .rejection_reason
                .as_ref()
                .unwrap()
                .contains("not eligible")
        );
    }

    #[test]
    fn read_modifiable_files_excludes_ineligible_domain_files() {
        let temp = tempfile::tempdir().unwrap();
        let core_src = temp.path().join("crates/a2d-core/src");
        fs::create_dir_all(&core_src).unwrap();
        fs::write(core_src.join("metabolism.rs"), "pub fn mechanism() {}").unwrap();
        fs::write(core_src.join("prime.rs"), "pub fn is_prime() {}").unwrap();
        fs::write(core_src.join("benchmark.rs"), "pub fn physics() {}").unwrap();

        let files = read_modifiable_files(temp.path());
        let paths = files.into_iter().map(|(path, _)| path).collect::<Vec<_>>();

        assert_eq!(paths, vec!["crates/a2d-core/src/metabolism.rs"]);
    }

    #[test]
    fn validate_patches_accepts_combined_production_and_test_change_atomically() {
        let temp = minimal_workspace_fixture();
        let prod_patch = SystemPatch {
            file_path: "crates/a2d-core/src/metabolism.rs".to_string(),
            new_content: "pub fn answer() -> i32 { 2 }\n".to_string(),
        };
        let test_patch = SystemPatch {
            file_path: "crates/a2d-core/tests/bootstrap.rs".to_string(),
            new_content: "use a2d_core::answer;\n\n#[test]\nfn answer_matches_contract() {\n    assert_eq!(answer(), 2);\n}\n"
                .to_string(),
        };

        assert!(!validate_patch(temp.path(), &prod_patch).accepted);
        assert!(!validate_patch(temp.path(), &test_patch).accepted);

        let result = validate_patches(temp.path(), &[prod_patch, test_patch]);

        assert!(
            result.accepted,
            "combined patch should pass; rejection: {:?}\nstderr: {}\nstdout: {}",
            result.rejection_reason, result.compile_output, result.test_output
        );
    }

    #[test]
    fn validate_patches_rejects_duplicate_targets_before_temp_apply() {
        let temp = minimal_workspace_fixture();
        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/metabolism.rs".to_string(),
            new_content: "pub fn answer() -> i32 { 2 }\n".to_string(),
        };

        let result = validate_patches(temp.path(), &[patch.clone(), patch]);

        assert!(!result.accepted);
        assert!(
            result
                .rejection_reason
                .as_deref()
                .unwrap_or_default()
                .contains("Duplicate SystemPatch target")
        );
    }

    fn minimal_workspace_fixture() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            r#"[workspace]
members = ["crates/a2d-core"]
resolver = "3"
"#,
        )
        .unwrap();
        let crate_dir = temp.path().join("crates/a2d-core");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::create_dir_all(crate_dir.join("tests")).unwrap();
        fs::write(
            crate_dir.join("Cargo.toml"),
            r#"[package]
name = "a2d-core"
version = "0.1.0"
edition = "2024"

[lib]
name = "a2d_core"
path = "src/metabolism.rs"
"#,
        )
        .unwrap();
        fs::write(
            crate_dir.join("src/metabolism.rs"),
            "pub fn answer() -> i32 { 1 }\n",
        )
        .unwrap();
        fs::write(
            crate_dir.join("tests/bootstrap.rs"),
            "use a2d_core::answer;\n\n#[test]\nfn answer_matches_contract() {\n    assert_eq!(answer(), 1);\n}\n",
        )
        .unwrap();
        temp
    }

    #[test]
    fn system_patch_serializes_roundtrip() {
        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/metabolism.rs".to_string(),
            new_content: "fn main() {}".to_string(),
        };
        let json = serde_json::to_string(&patch).unwrap();
        let roundtrip: SystemPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.file_path, patch.file_path);
        assert_eq!(roundtrip.new_content, patch.new_content);
    }

    /// Integration test: validates a real patch against the actual project.
    /// Slow (compiles from scratch in temp dir), so ignored by default.
    #[test]
    #[ignore]
    fn valid_patch_accepted_by_self_sandbox() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let project_root = project_root.canonicalize().unwrap();

        // Read current types.rs, add a harmless comment
        let types_path = project_root.join("crates/a2d-core/src/types.rs");
        let mut content = fs::read_to_string(&types_path).unwrap();
        content.push_str("\n// Self-modification test marker\n");

        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/types.rs".to_string(),
            new_content: content,
        };

        let result = validate_patch(&project_root, &patch);
        assert!(
            result.accepted,
            "Expected patch to be accepted. Rejection: {:?}\nCompile: {}\nTest: {}",
            result.rejection_reason, result.compile_output, result.test_output
        );
    }

    /// Integration test: validates that a breaking patch is rejected.
    #[test]
    #[ignore]
    fn breaking_patch_rejected_by_self_sandbox() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let project_root = project_root.canonicalize().unwrap();

        let patch = SystemPatch {
            file_path: "crates/a2d-core/src/types.rs".to_string(),
            new_content: "THIS IS NOT VALID RUST".to_string(),
        };

        let result = validate_patch(&project_root, &patch);
        assert!(!result.accepted);
        assert!(!result.compiled);
    }
}
