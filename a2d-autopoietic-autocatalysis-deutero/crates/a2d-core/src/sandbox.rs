//! Sandbox: compile and run Rust code in an isolated environment.
//!
//! This is what makes the tester enzyme real. Instead of asking an LLM
//! to evaluate code, we compile it with rustc and run the tests.
//! Mechanical verification, not conversational review.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Default test execution budget when a challenge doesn't specify one.
/// Generous — this is a sanity guard to prevent silent wedges, not a
/// performance ceiling.
pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Result of compiling and running code in the sandbox.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub compiled: bool,
    pub compile_output: String,
    pub tests_passed: Option<usize>,
    pub tests_failed: Option<usize>,
    pub test_output: String,
    pub runtime_output: String,
    /// True if the test binary was SIGKILLed after exceeding the test_timeout.
    /// When true, tests_passed/failed represent the synthetic "all failed" verdict.
    pub timed_out: bool,
    /// Wall-clock time spent running the test binary (None if tests didn't run).
    pub test_elapsed: Option<Duration>,
}

impl SandboxResult {
    /// Did the code compile and all tests pass?
    pub fn is_green(&self) -> bool {
        self.compiled && !self.timed_out && self.tests_failed == Some(0)
    }

    /// Mechanical fitness: tests_passed / (tests_passed + tests_failed)
    pub fn test_fitness(&self) -> f64 {
        match (self.tests_passed, self.tests_failed) {
            (Some(passed), Some(failed)) if passed + failed > 0 => {
                passed as f64 / (passed + failed) as f64
            }
            _ => 0.0,
        }
    }
}

/// Compile and test Rust code in a temporary directory.
///
/// The test binary is killed if it exceeds `test_timeout`. A killed test run
/// is reported as `timed_out=true` with synthetic failure counts (all tests
/// failed) so the coder receives a clear signal rather than an indefinite hang.
pub fn evaluate_rust_code(code: &str, test_timeout: Duration) -> SandboxResult {
    let dir = tempfile::tempdir().unwrap_or_else(|_| {
        tempfile::Builder::new()
            .prefix("a2d-sandbox-")
            .tempdir()
            .expect("failed to create temp dir")
    });

    let src_path = dir.path().join("main.rs");
    if let Err(e) = fs::write(&src_path, code) {
        return SandboxResult {
            compiled: false,
            compile_output: format!("Failed to write source: {e}"),
            tests_passed: None,
            tests_failed: None,
            test_output: String::new(),
            runtime_output: String::new(),
            timed_out: false,
            test_elapsed: None,
        };
    }

    // Try to compile
    let binary_path = dir.path().join("main");
    let compile = compile_rust(&src_path, &binary_path);

    if !compile.compiled {
        return compile;
    }

    // Try to compile with --test for test binary
    let test_binary = dir.path().join("test_main");
    let test_compile = Command::new("rustc")
        .args(["--test", "--edition", "2024"])
        .arg(&src_path)
        .arg("-o")
        .arg(&test_binary)
        .output();

    let (tests_passed, tests_failed, test_output, timed_out, test_elapsed) = match test_compile {
        Ok(output) if output.status.success() => {
            let run = run_with_timeout(Command::new(&test_binary), test_timeout);
            match run {
                TimedRun::Completed {
                    stdout,
                    stderr,
                    elapsed,
                } => {
                    let combined = format!("{stdout}\n{stderr}");
                    let (passed, failed) = parse_test_results(&combined);
                    (Some(passed), Some(failed), combined, false, Some(elapsed))
                }
                TimedRun::TimedOut {
                    limit,
                    elapsed,
                    partial_stdout,
                    partial_stderr,
                } => {
                    let diag = format!(
                        "TEST TIMEOUT: test binary exceeded {}s sanity guard (killed at {:.2}s). \
                         Your solution may have pathological runtime on some inputs — \
                         add input validation, early termination, or pruning.\n\n\
                         PARTIAL STDOUT:\n{}\n\nPARTIAL STDERR:\n{}",
                        limit.as_secs_f64(),
                        elapsed.as_secs_f64(),
                        partial_stdout,
                        partial_stderr,
                    );
                    // Synthetic all-failed verdict: report at least one failure,
                    // preserving any parse-visible passes from partial output.
                    let (passed, _) = parse_test_results(&partial_stdout);
                    (Some(passed), Some(1.max(1)), diag, true, Some(elapsed))
                }
                TimedRun::SpawnFailed(e) => {
                    (None, None, format!("Test run failed: {e}"), false, None)
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // Test compilation failed — code compiles but tests don't
            (
                Some(0),
                Some(1),
                format!("Test compilation failed:\n{stderr}"),
                false,
                None,
            )
        }
        Err(e) => (
            None,
            None,
            format!("rustc --test failed to start: {e}"),
            false,
            None,
        ),
    };

    SandboxResult {
        compiled: true,
        compile_output: compile.compile_output,
        tests_passed,
        tests_failed,
        test_output,
        runtime_output: String::new(),
        timed_out,
        test_elapsed,
    }
}

enum TimedRun {
    Completed {
        stdout: String,
        stderr: String,
        elapsed: Duration,
    },
    TimedOut {
        limit: Duration,
        elapsed: Duration,
        partial_stdout: String,
        partial_stderr: String,
    },
    SpawnFailed(std::io::Error),
}

/// Run a command with a wall-clock timeout. On timeout, SIGKILL the child and
/// reap it. Partial stdout/stderr captured when possible.
fn run_with_timeout(mut cmd: Command, limit: Duration) -> TimedRun {
    let start = Instant::now();
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => return TimedRun::SpawnFailed(e),
    };

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let elapsed = start.elapsed();
                return match child.wait_with_output() {
                    Ok(output) => TimedRun::Completed {
                        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                        elapsed,
                    },
                    Err(e) => TimedRun::SpawnFailed(e),
                };
            }
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    // wait_with_output consumes the child, but after kill+wait
                    // we can't call it. Partial output is best-effort via the
                    // now-dangling pipes the child dropped — in practice empty.
                    return TimedRun::TimedOut {
                        limit,
                        elapsed: start.elapsed(),
                        partial_stdout: String::new(),
                        partial_stderr: String::new(),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return TimedRun::SpawnFailed(e),
        }
    }
}

fn compile_rust(src: &Path, binary: &PathBuf) -> SandboxResult {
    let output = Command::new("rustc")
        .args(["--edition", "2024"])
        .arg(src)
        .arg("-o")
        .arg(binary)
        .output();

    match output {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            SandboxResult {
                compiled: output.status.success(),
                compile_output: stderr,
                tests_passed: None,
                tests_failed: None,
                test_output: String::new(),
                runtime_output: String::new(),
                timed_out: false,
                test_elapsed: None,
            }
        }
        Err(e) => SandboxResult {
            compiled: false,
            compile_output: format!("rustc not found or failed: {e}"),
            tests_passed: None,
            tests_failed: None,
            test_output: String::new(),
            runtime_output: String::new(),
            timed_out: false,
            test_elapsed: None,
        },
    }
}

/// Parse "test result: ok. 1 passed; 0 failed; ..." from rustc test output.
fn parse_test_results(output: &str) -> (usize, usize) {
    for line in output.lines() {
        if line.starts_with("test result:") {
            let mut passed = 0;
            let mut failed = 0;
            // Format: "test result: ok. 1 passed; 0 failed; 0 ignored; ..."
            // Split on ". " to get past "ok" or "FAILED", then split on ";"
            if let Some(stats) = line.split(". ").nth(1) {
                for part in stats.split(';') {
                    let trimmed = part.trim();
                    if let Some(n) = trimmed.strip_suffix(" passed") {
                        passed = n.trim().parse().unwrap_or(0);
                    }
                    if let Some(n) = trimmed.strip_suffix(" failed") {
                        failed = n.trim().parse().unwrap_or(0);
                    }
                }
            }
            return (passed, failed);
        }
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(code: &str) -> SandboxResult {
        evaluate_rust_code(code, DEFAULT_TEST_TIMEOUT)
    }

    #[test]
    fn valid_code_compiles() {
        let result = eval("fn main() { println!(\"hello\"); }");
        assert!(
            result.compiled,
            "Valid Rust should compile: {}",
            result.compile_output
        );
    }

    #[test]
    fn invalid_code_fails_compilation() {
        let result = eval("fn main( { }");
        assert!(!result.compiled);
    }

    #[test]
    fn code_with_passing_tests() {
        let code = r#"
fn is_prime(n: u64) -> bool {
    if n <= 1 { return false; }
    for i in 2..=(n as f64).sqrt() as u64 {
        if n % i == 0 { return false; }
    }
    true
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primes() {
        assert!(is_prime(2));
        assert!(is_prime(3));
        assert!(!is_prime(4));
        assert!(is_prime(5));
        assert!(!is_prime(1));
        assert!(!is_prime(0));
    }
}
"#;
        let result = eval(code);
        assert!(result.compiled);
        assert_eq!(result.tests_passed, Some(1));
        assert_eq!(result.tests_failed, Some(0));
        assert!(result.is_green());
    }

    #[test]
    fn code_with_failing_test() {
        let code = r#"
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn this_fails() {
        assert_eq!(1, 2);
    }
}
"#;
        let result = eval(code);
        assert!(result.compiled);
        assert_eq!(result.tests_failed, Some(1));
        assert!(!result.is_green());
    }

    #[test]
    fn test_fitness_calculation() {
        let result = SandboxResult {
            compiled: true,
            compile_output: String::new(),
            tests_passed: Some(3),
            tests_failed: Some(1),
            test_output: String::new(),
            runtime_output: String::new(),
            timed_out: false,
            test_elapsed: None,
        };
        assert!((result.test_fitness() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn slow_test_gets_killed_by_timeout() {
        // A test that sleeps 60s — with a 1s timeout it must be killed.
        let code = r#"
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn sleeps_forever() {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
"#;
        let result = evaluate_rust_code(code, Duration::from_secs(1));
        let test_elapsed = result
            .test_elapsed
            .expect("timed-out test run should record test binary elapsed time");

        assert!(result.compiled, "should compile");
        assert!(result.timed_out, "should have timed out");
        assert!(!result.is_green(), "timed-out runs are never green");
        assert!(
            test_elapsed < Duration::from_secs(10),
            "timeout should kill test binary promptly, took {test_elapsed:?}"
        );
        assert!(
            result.test_output.contains("TEST TIMEOUT"),
            "diagnostic should mention timeout"
        );
    }

    #[test]
    fn fast_test_not_affected_by_generous_timeout() {
        let code = r#"
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn fast() { assert!(true); }
}
"#;
        let result = evaluate_rust_code(code, Duration::from_secs(30));
        assert!(!result.timed_out);
        assert!(result.is_green());
    }

    #[test]
    fn parse_test_output() {
        let output = "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured";
        let (passed, failed) = parse_test_results(output);
        assert_eq!(passed, 5);
        assert_eq!(failed, 0);
    }
}
