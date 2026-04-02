//! Hidden sentinel suite — evaluation benchmarks the system cannot see.
//!
//! These exist from Stage 0 onward (per DESIGN.md v0.3.0). They are
//! the primary anti-Goodharting mechanism: if visible benchmark scores
//! diverge from sentinel scores, the system is gaming its evaluator.

fn command_spawn_error(
    command: &str,
    workspace_root: &std::path::Path,
    error: &std::io::Error,
) -> String {
    format!(
        "failed to launch `{command}` in `{}`: {error}",
        workspace_root.display()
    )
}

fn command_failure(
    command: &str,
    workspace_root: &std::path::Path,
    output: &std::process::Output,
) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    if detail.is_empty() {
        format!(
            "`{command}` failed in `{}` with status {}",
            workspace_root.display(),
            output.status
        )
    } else {
        format!(
            "`{command}` failed in `{}` with status {}: {}",
            workspace_root.display(),
            output.status,
            detail
        )
    }
}

/// Result of running the full sentinel suite.
#[derive(Clone, Debug)]
pub struct SuiteResult {
    pub results: Vec<SentinelResult>,
    pub all_passed: bool,
    pub score: f64,
}

/// A single sentinel benchmark.
pub struct Sentinel {
    pub name: String,
    pub description: String,
    /// The check function. Returns (passed, detail).
    check: Box<dyn Fn() -> (bool, String) + Send + Sync>,
}

impl Sentinel {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        check: impl Fn() -> (bool, String) + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            check: Box::new(check),
        }
    }

    pub fn run(&self) -> SentinelResult {
        let (passed, detail) = (self.check)();
        SentinelResult {
            name: self.name.clone(),
            passed,
            detail,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SentinelResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// The seed sentinel suite — Stage 0 hidden benchmarks.
pub struct SentinelSuite {
    sentinels: Vec<Sentinel>,
}

impl SentinelSuite {
    pub fn new() -> Self {
        Self {
            sentinels: Vec::new(),
        }
    }

    pub fn add(&mut self, sentinel: Sentinel) {
        self.sentinels.push(sentinel);
    }

    /// Run all sentinels. Returns individual results and overall pass/fail.
    pub fn run_all(&self) -> SuiteResult {
        let results: Vec<SentinelResult> = self.sentinels.iter().map(|s| s.run()).collect();
        let all_passed = results.iter().all(|r| r.passed);
        let score = if results.is_empty() {
            1.0
        } else {
            results.iter().filter(|r| r.passed).count() as f64 / results.len() as f64
        };

        SuiteResult {
            results,
            all_passed,
            score,
        }
    }

    /// Create the Stage 0 seed sentinel suite.
    /// These are intentionally simple — enough to catch obvious gaming.
    pub fn seed_suite(workspace_root: std::path::PathBuf) -> Self {
        let mut suite = Self::new();

        // Sentinel 1: workspace compiles.
        let root = workspace_root.clone();
        suite.add(Sentinel::new(
            "compile_check",
            "Workspace must compile without errors",
            move || {
                let output = std::process::Command::new("cargo")
                    .arg("check")
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() => (true, "cargo check passed".into()),
                    Ok(o) => (
                        false,
                        format!(
                            "`cargo check` failed in `{}` with status {}: {}",
                            root.display(),
                            o.status,
                            String::from_utf8_lossy(&o.stderr).trim()
                        ),
                    ),
                    Err(e) => (false, command_spawn_error("cargo check", &root, &e)),
                }
            },
        ));

        // Sentinel 2: tests pass.
        let root = workspace_root.clone();
        suite.add(Sentinel::new(
            "test_check",
            "All workspace tests must pass",
            move || {
                let output = std::process::Command::new("cargo")
                    .arg("test")
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() => (true, "cargo test passed".into()),
                    Ok(o) => (
                        false,
                        format!(
                            "`cargo test` failed in `{}` with status {}: {}",
                            root.display(),
                            o.status,
                            String::from_utf8_lossy(&o.stderr).trim()
                        ),
                    ),
                    Err(e) => (false, command_spawn_error("cargo test", &root, &e)),
                }
            },
        ));

        // Sentinel 3: no unsafe code (Stage 0 constraint).
        let root = workspace_root.clone();
        suite.add(Sentinel::new(
            "no_unsafe",
            "No unsafe blocks in crate source",
            move || {
                let output = std::process::Command::new("rg")
                    .args([
                        "-l",
                        "-g",
                        "*.rs",
                        r"unsafe\s*\{|unsafe\s+fn|unsafe\s+impl",
                        "crates/",
                    ])
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() && o.stdout.is_empty() => {
                        (true, "no unsafe found".into())
                    }
                    Ok(o) if o.status.code() == Some(1) && o.stdout.is_empty() => {
                        (true, "no unsafe found".into())
                    }
                    Ok(o) => (
                        false,
                        if !o.status.success() && o.status.code() != Some(1) {
                            command_failure(
                                "rg -l -g '*.rs' 'unsafe\\s*\\{|unsafe\\s+fn|unsafe\\s+impl' crates/",
                                &root,
                                &o,
                            )
                        } else {
                            format!(
                                "found unsafe Rust constructs under `{}`: {}",
                                root.display(),
                                String::from_utf8_lossy(&o.stdout).trim()
                            )
                        },
                    ),
                    Err(e) => (false, command_spawn_error("rg", &root, &e)),
                }
            },
        ));

        // Sentinel 4: clippy clean (A²-designed, human-applied).
        let root = workspace_root.clone();
        suite.add(Sentinel::new(
            "clippy_check",
            "Workspace must pass cargo clippy with no warnings",
            move || {
                let output = std::process::Command::new("cargo")
                    .args(["clippy", "--all-targets", "--", "-D", "warnings"])
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() => (true, "cargo clippy passed".into()),
                    Ok(o) => (
                        false,
                        format!(
                            "`cargo clippy --all-targets -- -D warnings` failed in `{}` with status {}: {}",
                            root.display(),
                            o.status,
                            String::from_utf8_lossy(&o.stderr).trim()
                        ),
                    ),
                    Err(e) => (
                        false,
                        command_spawn_error("cargo clippy --all-targets -- -D warnings", &root, &e),
                    ),
                }
            },
        ));

        // Sentinel 5: doc build clean — no rustdoc warnings.
        let root = workspace_root.clone();
        suite.add(Sentinel::new(
            "doc_check",
            "Workspace must build docs without warnings",
            move || {
                let output = std::process::Command::new("cargo")
                    .args(["doc", "--no-deps", "--document-private-items"])
                    .env("RUSTDOCFLAGS", "-D warnings")
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() => (true, "cargo doc passed".into()),
                    Ok(o) => (
                        false,
                        format!(
                            "`cargo doc --no-deps --document-private-items` failed in `{}` with status {}: {}",
                            root.display(),
                            o.status,
                            String::from_utf8_lossy(&o.stderr).trim()
                        ),
                    ),
                    Err(e) => (
                        false,
                        command_spawn_error("cargo doc --no-deps --document-private-items", &root, &e),
                    ),
                }
            },
        ));

        // Sentinel 6: Cargo.lock is present and up to date.
        let root = workspace_root;
        suite.add(Sentinel::new(
            "lockfile_check",
            "Cargo.lock must exist and match an offline lockfile regeneration",
            move || {
                let lockfile = root.join("Cargo.lock");
                if !lockfile.exists() {
                    return (
                        false,
                        format!("Cargo.lock is missing from `{}`", root.display()),
                    );
                }
                let original = match std::fs::read(&lockfile) {
                    Ok(contents) => contents,
                    Err(e) => {
                        return (
                            false,
                            format!("failed to read `{}`: {e}", lockfile.display()),
                        );
                    }
                };
                let output = std::process::Command::new("cargo")
                    .args(["generate-lockfile", "--offline"])
                    .current_dir(&root)
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        let regenerated = match std::fs::read(&lockfile) {
                            Ok(contents) => contents,
                            Err(e) => {
                                return (
                                    false,
                                    format!(
                                        "failed to read regenerated `{}`: {e}",
                                        lockfile.display()
                                    ),
                                );
                            }
                        };
                        if regenerated != original {
                            let _ = std::fs::write(&lockfile, &original);
                            (
                                false,
                                format!(
                                    "Cargo.lock is stale in `{}` — offline regeneration changes it",
                                    root.display(),
                                ),
                            )
                        } else {
                            (true, "Cargo.lock is present and up to date".into())
                        }
                    }
                    Ok(o) => (
                        false,
                        command_failure("cargo generate-lockfile --offline", &root, &o),
                    ),
                    Err(e) => (
                        false,
                        command_spawn_error("cargo generate-lockfile --offline", &root, &e),
                    ),
                }
            },
        ));

        suite
    }
}

impl Default for SentinelSuite {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn empty_suite_passes() {
        let suite = SentinelSuite::new();
        let result = suite.run_all();
        assert!(result.all_passed);
        assert!((result.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn passing_sentinel() {
        let mut suite = SentinelSuite::new();
        suite.add(Sentinel::new("always_pass", "test", || (true, "ok".into())));
        let result = suite.run_all();
        assert!(result.all_passed);
    }

    #[test]
    fn failing_sentinel_fails_suite() {
        let mut suite = SentinelSuite::new();
        suite.add(Sentinel::new("pass", "test", || (true, "ok".into())));
        suite.add(Sentinel::new("fail", "test", || (false, "bad".into())));
        let result = suite.run_all();
        assert!(!result.all_passed);
        assert!((result.score - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn command_failure_includes_status_without_stderr() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(2 << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let detail = command_failure("grep", std::path::Path::new("."), &output);
        assert!(detail.contains("status"));
        assert!(!detail.ends_with(':'));
    }

    #[test]
    fn command_failure_includes_stderr_when_present() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(2 << 8),
            stdout: Vec::new(),
            stderr: b"grep: invalid option -- P\n".to_vec(),
        };
        let detail = command_failure("grep", std::path::Path::new("."), &output);
        assert!(detail.contains("invalid option"));
    }
}
