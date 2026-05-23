//! Holdout Benchmark: external fitness measurement for the catalytic cycle.
//!
//! The benchmark suite lives outside the germline. Enzymes cannot see the
//! test cases — they only receive pass/fail counts after execution.
//! This is the mechanical fitness signal that makes mutation acceptance
//! meaningful (Stage 2, Constitution Invariant 2).

use serde::{Deserialize, Serialize};

/// A single benchmark case: input + expected output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub name: String,
    pub input: String,
    pub expected_output: String,
}

/// Result of running one benchmark case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub passed: bool,
}

/// Aggregate fitness from a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub fitness: f64,
    pub results: Vec<CaseResult>,
    /// Sandbox diagnostic output when fitness < 1.0.
    /// Contains compile errors and/or test failure output.
    /// Does NOT contain acceptance test source code (information barrier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

impl FitnessReport {
    /// Fitness ratio: passed / total. 0.0 to 1.0.
    pub fn compute(results: Vec<CaseResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let fitness = if total == 0 {
            0.0
        } else {
            passed as f64 / total as f64
        };
        Self {
            total,
            passed,
            failed,
            fitness,
            results,
            diagnostic: None,
        }
    }
}

/// A benchmark suite: a collection of cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub name: String,
    pub cases: Vec<BenchmarkCase>,
    /// Acceptance test code appended to the artifact before sandbox evaluation.
    /// The coder never sees this. "Does it do what it's supposed to do?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_test: Option<String>,
    /// Wall-clock limit on test binary execution. Sanity guard, not a
    /// performance ceiling — set generously per challenge. A killed run is
    /// reported as failed and the coder sees a TEST TIMEOUT diagnostic.
    #[serde(default = "default_test_timeout_secs")]
    pub test_timeout_secs: u64,
}

fn default_test_timeout_secs() -> u64 {
    30
}

impl Default for BenchmarkSuite {
    fn default() -> Self {
        Self {
            name: String::new(),
            cases: Vec::new(),
            acceptance_test: None,
            test_timeout_secs: default_test_timeout_secs(),
        }
    }
}

impl BenchmarkSuite {
    /// Evaluate code output against the benchmark.
    ///
    /// Two-phase evaluation:
    /// 1. Compile and run tests via sandbox (mechanical, real)
    /// 2. Fall back to string matching if code can't be extracted
    pub fn evaluate(&self, code_output: &str) -> FitnessReport {
        // Try to extract a Rust code block from the output
        let code = extract_rust_code(code_output);

        if let Some(mut code) = code {
            if let Some(ref acceptance) = self.acceptance_test {
                code = strip_module(&code, "a2d_acceptance");
                code.push_str("\n\n");
                code.push_str(acceptance);
            }

            // Phase 1: compile and run — the real fitness signal.
            // Timeout is a hard gate: over = fail. No partial credit for speed
            // (closes a gaming surface where shortcuts beat correctness).
            let sandbox_result = crate::sandbox::evaluate_rust_code(
                &code,
                std::time::Duration::from_secs(self.test_timeout_secs),
            );

            let mut results = Vec::new();

            results.push(CaseResult {
                name: "compiles".to_string(),
                passed: sandbox_result.compiled,
            });

            if sandbox_result.compiled {
                let tests_passed = sandbox_result.tests_passed.unwrap_or(0);
                let tests_exist =
                    sandbox_result.tests_passed.is_some() || sandbox_result.tests_failed.is_some();

                results.push(CaseResult {
                    name: "has_tests".to_string(),
                    passed: tests_exist && tests_passed > 0,
                });

                results.push(CaseResult {
                    name: "all_tests_pass".to_string(),
                    passed: sandbox_result.is_green(),
                });
            } else {
                results.push(CaseResult {
                    name: "has_tests".to_string(),
                    passed: false,
                });
                results.push(CaseResult {
                    name: "all_tests_pass".to_string(),
                    passed: false,
                });
            }

            // Also check string-based quality signals on the source
            for case in &self.cases {
                results.push(CaseResult {
                    name: case.name.clone(),
                    passed: code.contains(&case.expected_output),
                });
            }

            let mut report = FitnessReport::compute(results);

            // Capture sandbox diagnostics when not perfect.
            // The coder and evolver need to see WHY it failed.
            // Information barrier: we include compiler/test output but NOT
            // the acceptance test source code.
            if report.fitness < 1.0 {
                report.diagnostic = Some(format_sandbox_diagnostic(&sandbox_result));
            }

            report
        } else {
            // Phase 2: string matching fallback (no extractable code)
            let mut results: Vec<CaseResult> = vec![
                CaseResult {
                    name: "compiles".to_string(),
                    passed: false,
                },
                CaseResult {
                    name: "has_tests".to_string(),
                    passed: false,
                },
                CaseResult {
                    name: "all_tests_pass".to_string(),
                    passed: false,
                },
            ];

            for case in &self.cases {
                results.push(CaseResult {
                    name: case.name.clone(),
                    passed: code_output.contains(&case.expected_output),
                });
            }

            FitnessReport::compute(results)
        }
    }
}

/// Extract Rust code from LLM output (handles markdown fences).
/// Public alias for use by challenges module.
pub fn extract_rust_code_pub(output: &str) -> Option<String> {
    extract_rust_code(output)
}

/// Format sandbox output into a diagnostic string for the feedback loop.
/// Includes compiler errors and test failures but NOT acceptance test source.
fn format_sandbox_diagnostic(sandbox: &crate::sandbox::SandboxResult) -> String {
    let mut parts = Vec::new();

    if !sandbox.compiled {
        parts.push(format!("COMPILATION FAILED:\n{}", sandbox.compile_output));
    } else {
        let passed = sandbox.tests_passed.unwrap_or(0);
        let failed = sandbox.tests_failed.unwrap_or(0);
        if failed > 0 {
            parts.push(format!(
                "TESTS: {passed} passed, {failed} failed\nTEST OUTPUT:\n{}",
                sandbox.test_output
            ));
        } else if passed == 0 {
            parts.push("NO TESTS FOUND: code compiled but contains no test functions".to_string());
        }
    }

    if !sandbox.runtime_output.is_empty() {
        parts.push(format!("RUNTIME OUTPUT:\n{}", sandbox.runtime_output));
    }

    if parts.is_empty() {
        "Unknown failure — no diagnostic output captured".to_string()
    } else {
        parts.join("\n\n")
    }
}

fn strip_module(code: &str, module_name: &str) -> String {
    let pattern = format!("mod {module_name}");
    let mut result = String::with_capacity(code.len());
    let mut cursor = 0;

    for (i, _) in code.char_indices() {
        if i < cursor {
            continue;
        }

        if code[i..].starts_with(&pattern) {
            let rest = &code[i + pattern.len()..];
            let trimmed = rest.trim_start();
            if trimmed.starts_with('{') {
                let brace_start = i + pattern.len() + (rest.len() - trimmed.len());
                if let Some(end) = find_matching_brace(&code[brace_start..]) {
                    result.push_str(&code[cursor..i]);
                    cursor = brace_start + end + 1;
                }
            }
        }
    }

    result.push_str(&code[cursor..]);
    result
}

fn find_matching_brace(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    if bytes.first()? != &b'{' {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut in_char = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut escape_next = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                in_block_comment = false;
            }
            continue;
        }
        if b == b'\\' && (in_string || in_char) {
            escape_next = true;
            continue;
        }
        if b == b'"' && !in_char {
            in_string = !in_string;
            continue;
        }
        if b == b'\'' && !in_string {
            in_char = !in_char;
            continue;
        }
        if in_string || in_char {
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            in_line_comment = true;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            in_block_comment = true;
            continue;
        }
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn extract_rust_code(output: &str) -> Option<String> {
    // Try ```rust ... ``` first
    if let Some(start) = output.find("```rust") {
        let code_start = start + "```rust".len();
        if let Some(end) = output[code_start..].find("```") {
            return Some(output[code_start..code_start + end].trim().to_string());
        }
    }
    // Try ``` ... ```
    if let Some(start) = output.find("```\n") {
        let code_start = start + "```\n".len();
        if let Some(end) = output[code_start..].find("```") {
            let code = output[code_start..code_start + end].trim();
            if code.contains("fn ") {
                return Some(code.to_string());
            }
        }
    }
    // If the output itself looks like Rust code (starts with fn, use, pub, etc.)
    let trimmed = output.trim();
    if trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("//!")
    {
        return Some(trimmed.to_string());
    }
    None
}

/// The default benchmark suite for the seed system.
pub fn seed_benchmark() -> BenchmarkSuite {
    BenchmarkSuite {
        name: "seed-benchmark-v1".to_string(),
        acceptance_test: None,
        test_timeout_secs: 30,
        cases: vec![
            BenchmarkCase {
                name: "function_exists".to_string(),
                input: "Check that the code defines a function".to_string(),
                expected_output: "fn ".to_string(),
            },
            BenchmarkCase {
                name: "has_tests".to_string(),
                input: "Check that the code includes tests".to_string(),
                expected_output: "#[test]".to_string(),
            },
            BenchmarkCase {
                name: "has_return_type".to_string(),
                input: "Check that functions have return types".to_string(),
                expected_output: "-> ".to_string(),
            },
            BenchmarkCase {
                name: "no_unwrap".to_string(),
                input: "Check for absence of unwrap (prefer Result)".to_string(),
                expected_output: "Result".to_string(),
            },
            BenchmarkCase {
                name: "has_doc_comment".to_string(),
                input: "Check for documentation".to_string(),
                expected_output: "///".to_string(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compilable_code_scores_compiles_check() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        // "fn main() {}" is extractable Rust, compiles, but no tests
        let report = suite.evaluate("fn main() {}");
        let compiles = report.results.iter().find(|r| r.name == "compiles");
        assert!(compiles.is_some_and(|c| c.passed));
    }

    #[test]
    fn non_rust_fails_all_sandbox_checks() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        let report = suite.evaluate("print('hello')");
        // Can't extract Rust → compiles=false, has_tests=false, all_tests_pass=false
        assert_eq!(report.passed, 0);
    }

    #[test]
    fn code_with_passing_tests_scores_high() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() {}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() { assert_eq!(add(1, 2), 3); }\n}";
        let report = suite.evaluate(code);
        // compiles + has_tests + all_tests_pass
        assert!(
            report.passed >= 3,
            "Expected >=3, got {}: {:?}",
            report.passed,
            report.results
        );
    }

    #[test]
    fn markdown_fenced_code_extracted() {
        let output = "Here's the code:\n```rust\nfn main() {}\n```\nDone.";
        let code = extract_rust_code(output);
        assert!(code.is_some());
        assert!(code.unwrap().contains("fn main"));
    }

    #[test]
    fn seed_benchmark_has_cases() {
        let suite = seed_benchmark();
        assert_eq!(suite.cases.len(), 5);
    }

    #[test]
    fn diagnostic_populated_on_compile_failure() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        let report = suite.evaluate("fn main() { let x: i32 = \"nope\"; }");
        assert!(report.fitness < 1.0);
        assert!(report.diagnostic.is_some());
        let diag = report.diagnostic.unwrap();
        assert!(
            diag.contains("COMPILATION FAILED"),
            "expected compile failure diagnostic, got: {diag}"
        );
    }

    #[test]
    fn diagnostic_populated_on_test_failure() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() {}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_bad() { assert_eq!(add(1, 2), 999); }\n}";
        let report = suite.evaluate(code);
        assert!(report.fitness < 1.0);
        assert!(report.diagnostic.is_some());
        let diag = report.diagnostic.unwrap();
        assert!(
            diag.contains("TESTS:") && diag.contains("failed"),
            "expected test failure diagnostic, got: {diag}"
        );
    }

    #[test]
    fn strip_duplicate_acceptance_module() {
        let code = "fn main() {}\n\n#[cfg(test)]\nmod a2d_acceptance {\n    use super::*;\n    #[test]\n    fn foo() {}\n}\n";
        let stripped = strip_module(code, "a2d_acceptance");
        assert!(
            !stripped.contains("a2d_acceptance"),
            "module should be removed: {stripped}"
        );
        assert!(
            stripped.contains("fn main()"),
            "other code preserved: {stripped}"
        );
    }

    #[test]
    fn strip_preserves_other_modules() {
        let code = "fn main() {}\n\nmod tests {\n    fn inner() {}\n}\n\nmod a2d_acceptance {\n    fn bad() {}\n}\n";
        let stripped = strip_module(code, "a2d_acceptance");
        assert!(!stripped.contains("a2d_acceptance"));
        assert!(stripped.contains("mod tests"));
        assert!(stripped.contains("fn inner"));
    }

    #[test]
    fn strip_module_with_nested_braces() {
        let code = "fn main() {}\nmod a2d_acceptance {\n    fn outer() {\n        if true {\n            let x = \"{test}\";\n        }\n    }\n}\n";
        let stripped = strip_module(code, "a2d_acceptance");
        assert!(!stripped.contains("a2d_acceptance"));
        assert!(stripped.contains("fn main()"));
    }

    #[test]
    fn strip_module_noop_when_absent() {
        let code = "fn main() {}\nmod tests { fn t() {} }\n";
        let stripped = strip_module(code, "a2d_acceptance");
        assert_eq!(stripped, code);
    }

    #[test]
    fn strip_module_handles_unicode_source() {
        let code = "fn multiply_symbol() -> char { '×' }\nmod a2d_acceptance { fn hidden() {} }\n";
        let stripped = strip_module(code, "a2d_acceptance");
        assert!(stripped.contains("'×'"));
        assert!(!stripped.contains("a2d_acceptance"));
    }

    #[test]
    fn no_diagnostic_on_perfect_fitness() {
        let suite = BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            ..Default::default()
        };
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() {}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() { assert_eq!(add(1, 2), 3); }\n}";
        let report = suite.evaluate(code);
        assert_eq!(report.fitness, 1.0);
        assert!(report.diagnostic.is_none());
    }
}
