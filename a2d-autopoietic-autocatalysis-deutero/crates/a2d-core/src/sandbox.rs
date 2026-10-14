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
    let mut test_compile_command = Command::new("rustc");
    test_compile_command
        .args(["--test", "--edition", "2024"])
        .arg(&src_path)
        .arg("-o")
        .arg(&test_binary);
    harden_sandbox_child_command(&mut test_compile_command);
    let test_compile = test_compile_command.output();

    let (tests_passed, tests_failed, test_output, timed_out, test_elapsed) = match test_compile {
        Ok(output) if output.status.success() => {
            let mut test_command = Command::new(&test_binary);
            harden_sandbox_child_command(&mut test_command);
            let run = run_with_timeout(test_command, test_timeout);
            match run {
                TimedRun::Completed {
                    stdout,
                    stderr,
                    elapsed,
                    ..
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

/// Outcome of [`run_isolated_tests`].
#[derive(Debug, Clone)]
pub struct IsolatedTestRun {
    pub compiled: bool,
    pub passed: usize,
    pub failed: usize,
    /// The test binary exited successfully (libtest exits non-zero on any
    /// failure or panic).
    pub exit_success: bool,
    pub timed_out: bool,
    /// Compiler output if compilation failed, otherwise test output.
    pub output: String,
}

/// Compile a crate from two files and run only the tests whose names
/// contain `filter`.
///
/// `root` is the crate root, written as `root_file`; `module` is written next
/// to it as `module_file` and must be referenced by the root (e.g. a
/// `#[path]` module holding trusted tests). After compilation both sources
/// are deleted and the binary runs from a separate empty directory, so code
/// under test cannot read the sources at run time. Compilation and execution
/// each have a timeout.
pub fn run_isolated_tests(
    root: &str,
    module: &str,
    root_file: &str,
    module_file: &str,
    filter: &str,
    compile_timeout: Duration,
    test_timeout: Duration,
) -> IsolatedTestRun {
    let failed = |output: String, timed_out: bool| IsolatedTestRun {
        compiled: false,
        passed: 0,
        failed: 0,
        exit_success: false,
        timed_out,
        output,
    };
    let (Ok(build_dir), Ok(run_dir)) = (
        tempfile::Builder::new().prefix("a2d-oracle-").tempdir(),
        tempfile::Builder::new().prefix("a2d-oracle-run-").tempdir(),
    ) else {
        return failed("Failed to create temp dirs".to_string(), false);
    };
    let harness_path = build_dir.path().join(root_file);
    let candidate_path = build_dir.path().join(module_file);
    if fs::write(&harness_path, root).is_err() || fs::write(&candidate_path, module).is_err() {
        return failed("Failed to write sources".to_string(), false);
    }

    let binary = build_dir.path().join("oracle_bin");
    let mut compile = Command::new("rustc");
    compile
        .args(["--test", "--edition", "2024"])
        .arg(&harness_path)
        .arg("-o")
        .arg(&binary)
        .current_dir(build_dir.path());
    harden_sandbox_child_command(&mut compile);
    match run_with_timeout(compile, compile_timeout) {
        TimedRun::Completed { success: true, .. } => {}
        TimedRun::Completed { stderr, .. } => return failed(stderr, false),
        TimedRun::TimedOut { limit, .. } => {
            return failed(format!("COMPILE TIMEOUT after {}s", limit.as_secs()), true);
        }
        TimedRun::SpawnFailed(e) => return failed(format!("rustc failed to start: {e}"), false),
    }
    let _ = fs::remove_file(&harness_path);
    let _ = fs::remove_file(&candidate_path);

    let mut run = Command::new(&binary);
    run.arg(filter).current_dir(run_dir.path());
    harden_sandbox_child_command(&mut run);
    match run_with_timeout(run, test_timeout) {
        TimedRun::Completed {
            stdout,
            stderr,
            success,
            ..
        } => {
            let output = format!("{stdout}\n{stderr}");
            let (passed, failed) = parse_last_test_results(&output);
            IsolatedTestRun {
                compiled: true,
                passed,
                failed,
                exit_success: success,
                timed_out: false,
                output,
            }
        }
        TimedRun::TimedOut { limit, elapsed, .. } => IsolatedTestRun {
            compiled: true,
            passed: 0,
            failed: 0,
            exit_success: false,
            timed_out: true,
            output: format!(
                "TEST TIMEOUT: test binary exceeded {}s (killed at {:.2}s)",
                limit.as_secs_f64(),
                elapsed.as_secs_f64()
            ),
        },
        TimedRun::SpawnFailed(e) => IsolatedTestRun {
            compiled: true,
            passed: 0,
            failed: 0,
            exit_success: false,
            timed_out: false,
            output: format!("Test run failed: {e}"),
        },
    }
}

/// Compile `library` as a separate rlib crate named `lib_name`, then
/// compile `tests` (a crate root that should `use lib_name::*;`) as a test
/// crate linked against it. Only the library's public items are reachable,
/// unlike the oracle's child-module layout. Nothing is run. `Err` carries
/// the compiler output.
pub fn compile_tests_against_library(
    library: &str,
    tests: &str,
    lib_name: &str,
    compile_timeout: Duration,
) -> Result<(), String> {
    let dir = tempfile::Builder::new()
        .prefix("a2d-api-")
        .tempdir()
        .map_err(|e| format!("temp dir: {e}"))?;
    let lib_path = dir.path().join("lib.rs");
    let tests_path = dir.path().join("tests.rs");
    fs::write(&lib_path, library).map_err(|e| e.to_string())?;
    fs::write(&tests_path, tests).map_err(|e| e.to_string())?;
    let rlib = dir.path().join(format!("lib{lib_name}.rlib"));
    let compile = |mut command: Command| -> Result<(), String> {
        harden_sandbox_child_command(&mut command);
        match run_with_timeout(command, compile_timeout) {
            TimedRun::Completed { success: true, .. } => Ok(()),
            TimedRun::Completed { stderr, .. } => Err(stderr),
            TimedRun::TimedOut { limit, .. } => {
                Err(format!("compile timeout after {}s", limit.as_secs()))
            }
            TimedRun::SpawnFailed(e) => Err(format!("rustc failed to start: {e}")),
        }
    };
    let mut lib = Command::new("rustc");
    lib.args([
        "--edition",
        "2024",
        "--crate-type",
        "lib",
        "--crate-name",
        lib_name,
    ])
    .arg(&lib_path)
    .arg("-o")
    .arg(&rlib)
    .current_dir(dir.path());
    compile(lib).map_err(|e| format!("library: {e}"))?;
    let mut test = Command::new("rustc");
    test.args(["--test", "--edition", "2024", "--extern"])
        .arg(format!("{lib_name}={}", rlib.display()))
        .arg(&tests_path)
        .arg("-o")
        .arg(dir.path().join("tests_bin"))
        .current_dir(dir.path());
    compile(test)
}

fn parse_last_test_results(output: &str) -> (usize, usize) {
    output
        .lines()
        .rev()
        .find(|line| line.starts_with("test result:"))
        .map(parse_test_results)
        .unwrap_or((0, 0))
}

enum TimedRun {
    Completed {
        stdout: String,
        stderr: String,
        success: bool,
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

fn spawn_pipe_reader<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
) -> Option<std::thread::JoinHandle<Vec<u8>>> {
    pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            buffer
        })
    })
}

fn join_pipe_reader(reader: Option<std::thread::JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Run a command with a wall-clock timeout. On timeout, SIGKILL the child and
/// reap it. Both pipes are drained while waiting, so a child that writes more
/// than a pipe buffer cannot block and be misreported as timed out.
fn run_with_timeout(mut cmd: Command, limit: Duration) -> TimedRun {
    let start = Instant::now();
    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => return TimedRun::SpawnFailed(e),
    };
    let stdout_reader = spawn_pipe_reader(child.stdout.take());
    let stderr_reader = spawn_pipe_reader(child.stderr.take());

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return TimedRun::Completed {
                    stdout: join_pipe_reader(stdout_reader),
                    stderr: join_pipe_reader(stderr_reader),
                    success: status.success(),
                    elapsed: start.elapsed(),
                };
            }
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Readers are not joined: a grandchild may still hold the
                    // pipes open. Partial output is not reported.
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

fn harden_sandbox_child_command(command: &mut Command) {
    crate::process_env::apply_no_public_solution_search_env(command);
    crate::process_env::remove_network_configuration_env(command);
}

fn compile_rust(src: &Path, binary: &PathBuf) -> SandboxResult {
    let mut command = Command::new("rustc");
    command
        .args(["--edition", "2024"])
        .arg(src)
        .arg("-o")
        .arg(binary);
    harden_sandbox_child_command(&mut command);
    let output = command.output();

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

    #[test]
    fn sandbox_child_process_env_fixture() {
        if std::env::var("A2D_RUN_SANDBOX_ENV_FIXTURE").as_deref() != Ok("1") {
            return;
        }

        let network_keys = crate::process_env::network_configuration_env_vars()
            .into_iter()
            .map(|key| format!("        assert!(std::env::var({key:?}).is_err(), {key:?});"))
            .collect::<Vec<_>>()
            .join("\n");
        let policy_assertions = crate::process_env::no_public_solution_search_env()
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "        assert_eq!(std::env::var({key:?}).as_deref(), Ok({value:?}), {key:?});"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let code = format!(
            r#"
fn main() {{}}

#[cfg(test)]
mod tests {{
    #[test]
    fn generated_test_observes_sandbox_child_env() {{
{policy_assertions}
{network_keys}
    }}
}}
"#,
        );

        let result = evaluate_rust_code(&code, DEFAULT_TEST_TIMEOUT);
        assert!(
            result.compiled,
            "generated env probe should compile: {}",
            result.compile_output
        );
        assert!(
            result.is_green(),
            "generated env probe should pass; test output:\n{}",
            result.test_output
        );
    }

    #[test]
    fn sandbox_evaluated_code_receives_policy_env_and_scrubbed_network_env() {
        let current_exe = std::env::current_exe().expect("current test binary path");
        let mut command = Command::new(current_exe);
        command
            .arg("--exact")
            .arg("sandbox::tests::sandbox_child_process_env_fixture")
            .arg("--nocapture")
            .env("A2D_RUN_SANDBOX_ENV_FIXTURE", "1");
        for key in crate::process_env::network_configuration_env_vars() {
            command.env(key, format!("sandbox-leak-sentinel-{key}"));
        }

        let output = command
            .output()
            .expect("fixture test process should execute");
        assert!(
            output.status.success(),
            "fixture failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn isolated(harness: &str, candidate: &str, filter: &str) -> IsolatedTestRun {
        run_isolated_tests(
            harness,
            candidate,
            "h_test.rs",
            "c_test.rs",
            filter,
            Duration::from_secs(120),
            DEFAULT_TEST_TIMEOUT,
        )
    }

    const HARNESS: &str = "#[cfg(test)]\nmod oracle_x1 {\n    use super::*;\n    #[test]\n    fn adds() { assert_eq!(add(2, 2), 4); }\n    #[test]\n    fn adds_negative() { assert_eq!(add(-1, 1), 0); }\n}\ninclude!(\"c_test.rs\");\n";

    #[test]
    fn isolated_run_counts_only_matching_module_and_needs_clean_exit() {
        let candidate = "fn add(a: i32, b: i32) -> i32 { a + b }\n#[cfg(test)]\nmod own {\n    #[test]\n    fn candidate_test_fails() { panic!(\"own\"); }\n}\n";
        let result = isolated(HARNESS, candidate, "oracle_x1::");
        assert!(result.compiled, "{}", result.output);
        assert_eq!((result.passed, result.failed), (2, 0), "{}", result.output);
        assert!(result.exit_success);

        let wrong = isolated(
            HARNESS,
            "fn add(a: i32, b: i32) -> i32 { a - b }\n",
            "oracle_x1::",
        );
        assert_eq!((wrong.passed, wrong.failed), (0, 2), "{}", wrong.output);
        assert!(!wrong.exit_success);
    }

    #[test]
    fn isolated_run_reports_compile_failure() {
        let result = isolated(HARNESS, "fn broken( {", "oracle_x1::");
        assert!(!result.compiled);
        assert!(!result.exit_success);
    }

    #[test]
    fn candidate_cannot_read_harness_at_run_time() {
        let harness = "#[cfg(test)]\nmod oracle_r1 {\n    use super::*;\n    #[test]\n    fn harness_is_gone() { assert!(!probe()); }\n}\ninclude!(\"c_test.rs\");\n";
        let candidate = "fn probe() -> bool { std::path::Path::new(\"h_test.rs\").exists() || std::fs::read_dir(\".\").map(|d| d.count() > 0).unwrap_or(false) }\n";
        let result = isolated(harness, candidate, "oracle_r1::");
        assert_eq!((result.passed, result.failed), (1, 0), "{}", result.output);
    }
}
