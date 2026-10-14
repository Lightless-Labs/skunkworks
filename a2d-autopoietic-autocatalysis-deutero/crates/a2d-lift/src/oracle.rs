//! Public and hidden oracles.
//!
//! Each oracle appends its tests to the candidate as a `#[cfg(test)]` module
//! with a per-run unguessable name, runs only that module's tests, and
//! requires every one of them to pass.

use crate::ledger::{sha256_hex, unix_ms};
use crate::task::{PublicTask, Task, count_tests};
use a2d_core::sandbox::run_rust_tests_matching;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestScore {
    pub compiled: bool,
    pub passed: usize,
    pub failed: usize,
    pub expected: usize,
    pub timed_out: bool,
    pub all_passed: bool,
    #[serde(skip)]
    pub output: String,
}

impl TestScore {
    pub fn missing(expected: usize) -> Self {
        Self {
            compiled: false,
            passed: 0,
            failed: 0,
            expected,
            timed_out: false,
            all_passed: false,
            output: "no extractable Rust code".to_string(),
        }
    }
}

fn module_name(label: &str, code: &str) -> String {
    let seed = format!(
        "{label}:{}:{}:{}:{}",
        unix_ms(),
        std::process::id(),
        NONCE_COUNTER.fetch_add(1, Ordering::Relaxed),
        sha256_hex(code.as_bytes())
    );
    format!("a2d_{label}_{}", &sha256_hex(seed.as_bytes())[..16])
}

/// Score `code` against `tests` (test functions, without a module wrapper).
pub fn score_tests(code: &str, tests: &str, label: &str, timeout_secs: u64) -> TestScore {
    let expected = count_tests(tests);
    let module = module_name(label, code);
    let program = format!(
        "{code}\n\n#[cfg(test)]\n#[allow(unused_imports, dead_code)]\nmod {module} {{\n    use super::*;\n{tests}\n}}\n"
    );
    let result = run_rust_tests_matching(
        &program,
        &format!("{module}::"),
        Duration::from_secs(timeout_secs),
    );
    let passed = result.tests_passed.unwrap_or(0);
    let failed = result.tests_failed.unwrap_or(0);
    let output = if result.compiled {
        result.test_output
    } else {
        result.compile_output
    };
    TestScore {
        compiled: result.compiled,
        passed,
        failed,
        expected,
        timed_out: result.timed_out,
        all_passed: result.compiled
            && !result.timed_out
            && failed == 0
            && expected > 0
            && passed == expected,
        output,
    }
}

/// The evaluation oracle. Scores a final submission once.
pub fn score_hidden(task: &Task, code: Option<&str>) -> TestScore {
    let expected = count_tests(task.hidden_tests());
    match code {
        Some(code) => score_tests(code, task.hidden_tests(), "hidden", task.test_timeout_secs),
        None => TestScore::missing(expected),
    }
}

/// The selection oracle: the only signal an arm may use while working.
/// `None` when the task has no public tests.
pub fn score_public(task: &PublicTask, code: Option<&str>) -> Option<TestScore> {
    let expected = count_tests(&task.public_tests);
    if expected == 0 {
        return None;
    }
    Some(match code {
        Some(code) => score_tests(code, &task.public_tests, "public", task.test_timeout_secs),
        None => TestScore::missing(expected),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "fn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n";
    const TESTS: &str = "#[test]\nfn doubles() { assert_eq!(double(2), 4); }\n#[test]\nfn doubles_zero() { assert_eq!(double(0), 0); }\n";

    #[test]
    fn all_passed_requires_every_expected_test() {
        let score = score_tests(GOOD, TESTS, "hidden", 10);
        assert!(score.all_passed, "{}", score.output);
        assert_eq!((score.passed, score.expected), (2, 2));
        let wrong = score_tests("fn double(x: i32) -> i32 { x }\n", TESTS, "hidden", 10);
        assert!(!wrong.all_passed);
        assert_eq!(wrong.passed, 1);
        assert_eq!(wrong.failed, 1);
    }

    #[test]
    fn candidate_cannot_spoof_or_collide_with_oracle_module() {
        // A candidate that defines its own a2d_hidden module with passing
        // tests gains nothing: the oracle's module name carries a nonce.
        let spoof = "fn double(x: i32) -> i32 { x }\n#[cfg(test)]\nmod a2d_hidden {\n    #[test]\n    fn doubles() {}\n    #[test]\n    fn doubles_zero() {}\n}\n";
        let score = score_tests(spoof, TESTS, "hidden", 10);
        assert!(!score.all_passed);
        assert_eq!(score.passed, 1);
    }

    #[test]
    fn compile_failure_and_missing_code_fail() {
        let score = score_tests("fn double(", TESTS, "hidden", 10);
        assert!(!score.compiled && !score.all_passed);
        let task = Task::for_test("t", "f", "", TESTS);
        let missing = score_hidden(&task, None);
        assert!(!missing.all_passed);
        assert_eq!(missing.expected, 2);
        assert!(score_public(&task.public(), Some(GOOD)).is_none());
    }
}
