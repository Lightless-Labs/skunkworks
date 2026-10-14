//! Public and hidden oracles.
//!
//! The candidate is the crate root. The oracle's tests live in a separate,
//! randomly named file, declared as a `#[cfg(test)] #[path]` module *before*
//! the candidate's code:
//! - candidate `macro_rules!` cannot reach the tests (textual scope runs
//!   forward), and the module imports the assertion macros explicitly so a
//!   candidate `#[macro_export]` cannot shadow them through `use super::*`;
//! - `include_str!(file!())` in the candidate yields only its own source and
//!   a file name it cannot `include_str!` at compile time (no dynamic paths);
//! - sources are deleted before the tests run.
//!
//! A test run counts only if the binary exits successfully with every
//! expected test passed.

use crate::ledger::{sha256_hex, unix_ms};
use crate::task::{PublicTask, Task, count_tests};
use a2d_core::sandbox::run_isolated_tests;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);

const COMPILE_TIMEOUT_SECS: u64 = 120;

/// Explicit imports beat glob imports, so these resolve to std even if the
/// candidate exports macros with the same names.
const TRUSTED_MACROS: &str = "use ::std::{assert, assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, matches, unreachable};";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestScore {
    pub compiled: bool,
    pub passed: usize,
    pub failed: usize,
    pub expected: usize,
    pub timed_out: bool,
    pub exit_success: bool,
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
            exit_success: false,
            all_passed: false,
            output: "no extractable Rust code".to_string(),
        }
    }
}

fn nonce(label: &str, code: &str) -> String {
    let seed = format!(
        "{label}:{}:{}:{}:{}",
        unix_ms(),
        std::process::id(),
        NONCE_COUNTER.fetch_add(1, Ordering::Relaxed),
        sha256_hex(code.as_bytes())
    );
    sha256_hex(seed.as_bytes())[..16].to_string()
}

/// Split leading inner attributes (`#![...]`, `//!`) off the candidate: they
/// are only valid at the crate root, which is now the harness.
fn split_inner_attributes(code: &str) -> (String, String) {
    let mut attributes = Vec::new();
    let mut lines = code.lines().peekable();
    let mut open_brackets = 0i32;
    while let Some(line) = lines.peek() {
        let trimmed = line.trim();
        let continuing = open_brackets > 0;
        if continuing || trimmed.starts_with("#![") {
            open_brackets += trimmed.matches('[').count() as i32;
            open_brackets -= trimmed.matches(']').count() as i32;
            attributes.push(*line);
        } else if trimmed.is_empty() || trimmed.starts_with("//") {
            // Inner docs (`//!`), plain comments and blank lines.
            attributes.push(*line);
        } else {
            break;
        }
        lines.next();
    }
    let body: Vec<&str> = lines.collect();
    (attributes.join("\n"), body.join("\n"))
}

/// Score `code` against `tests` (test functions, without a module wrapper).
pub fn score_tests(code: &str, tests: &str, label: &str, timeout_secs: u64) -> TestScore {
    let expected = count_tests(tests);
    let nonce = nonce(label, code);
    let module = format!("a2d_{label}_{nonce}");
    let tests_file = format!("t_{nonce}.rs");
    let (attributes, body) = split_inner_attributes(code);
    let root = format!(
        "{attributes}\n#[cfg(test)]\n#[path = \"{tests_file}\"]\nmod {module};\n\n{body}\n"
    );
    let tests_module = format!(
        "#![allow(unused_imports, dead_code)]\nuse super::*;\n{TRUSTED_MACROS}\n\n{tests}\n"
    );
    let run = run_isolated_tests(
        &root,
        &tests_module,
        &format!("c_{nonce}.rs"),
        &tests_file,
        &format!("{module}::"),
        Duration::from_secs(COMPILE_TIMEOUT_SECS),
        Duration::from_secs(timeout_secs),
    );
    TestScore {
        compiled: run.compiled,
        passed: run.passed,
        failed: run.failed,
        expected,
        timed_out: run.timed_out,
        exit_success: run.exit_success,
        all_passed: run.compiled
            && run.exit_success
            && !run.timed_out
            && run.failed == 0
            && expected > 0
            && run.passed == expected,
        output: run.output,
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
    fn candidate_macros_cannot_disable_oracle_assertions() {
        let textual =
            "macro_rules! assert_eq { ($($t:tt)*) => {}; }\nfn double(x: i32) -> i32 { x }\n";
        let exported = "#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => {}; }\nfn double(x: i32) -> i32 { x }\n";
        for candidate in [textual, exported] {
            let score = score_tests(candidate, TESTS, "hidden", 10);
            assert!(
                !score.all_passed,
                "assertions disabled by:\n{candidate}\n{}",
                score.output
            );
        }
    }

    #[test]
    fn candidate_cannot_read_oracle_source_at_compile_time() {
        let tests = "#[test]\nfn sees_nothing() { assert!(!leaked()); }\n";
        let candidate = "fn leaked() -> bool { include_str!(file!()).contains(&[\"sees\", \"_nothing\"].concat()) }\n";
        let score = score_tests(candidate, tests, "hidden", 10);
        assert!(score.all_passed, "{}", score.output);
    }

    #[test]
    fn forged_summary_with_failing_exit_does_not_pass() {
        let candidate = "fn double(x: i32) -> i32 {\n    eprintln!(\"test result: ok. 2 passed; 0 failed; 0 ignored\");\n    x\n}\n";
        let score = score_tests(candidate, TESTS, "hidden", 10);
        assert!(!score.all_passed, "{}", score.output);
    }

    #[test]
    fn leading_inner_attributes_and_docs_are_hoisted() {
        let candidate = "//! Doubler.\n#![allow(dead_code)]\n#![allow(\n    unused_variables\n)]\n\nfn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n";
        let score = score_tests(candidate, TESTS, "hidden", 10);
        assert!(score.all_passed, "{}", score.output);
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
