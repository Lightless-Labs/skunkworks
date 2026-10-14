//! Oracle validation (`a2d lift validate`): a task is accepted only if its
//! independent reference passes every public and hidden test, every mutant
//! fails at least one hidden test, at least one mutant passes the public
//! tests (so hidden tests discriminate beyond them), and hidden tests do not
//! repeat public test names, and the public tests call every `pub fn` the
//! requirements name. Verdicts carry counts, file stems and requirement
//! (public) names only, never test content, so holdout sets can be
//! validated blind.

use crate::oracle::{score_hidden, score_public};
use crate::task::{Task, load_suite};
use a2d_core::sandbox::compile_tests_against_library;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub const REFERENCE_FILE: &str = "reference.rs";
pub const MUTANTS_DIR: &str = "mutants";
/// Authoring-spec minimums (P1 authoring spec v2).
pub const MIN_MUTANTS: usize = 4;
pub const MIN_PUBLIC_GREEN_MUTANTS: usize = 2;
pub const MIN_HIDDEN_TESTS: usize = 20;
pub const MIN_PUBLIC_TESTS: usize = 3;

#[derive(Debug, Clone)]
pub struct TaskVerdict {
    pub task: String,
    pub problems: Vec<String>,
    pub mutants: usize,
    pub mutants_caught: usize,
    pub mutants_public_green: usize,
}

impl TaskVerdict {
    pub fn accepted(&self) -> bool {
        self.problems.is_empty()
    }
}

impl fmt::Display for TaskVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.accepted() {
            write!(
                f,
                "{}: OK (mutants caught by hidden {}/{}, passing public {})",
                self.task, self.mutants_caught, self.mutants, self.mutants_public_green
            )
        } else {
            write!(f, "{}: REJECT {}", self.task, self.problems.join("; "))
        }
    }
}

pub struct SuiteReport {
    pub hash: String,
    pub verdicts: Vec<TaskVerdict>,
}

pub fn validate_suite(dir: &Path, jobs: usize) -> Result<SuiteReport, String> {
    let suite = load_suite(dir, None)?;
    let next = AtomicUsize::new(0);
    let verdicts = Mutex::new(Vec::new());
    std::thread::scope(|workers| {
        for _ in 0..jobs.max(1) {
            workers.spawn(|| {
                while let Some(task) = suite.tasks.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let verdict = validate_task(task);
                    verdicts.lock().unwrap().push(verdict);
                }
            });
        }
    });
    let mut verdicts = verdicts.into_inner().unwrap();
    verdicts.sort_by(|a, b| a.task.cmp(&b.task));
    Ok(SuiteReport {
        hash: suite.hash,
        verdicts,
    })
}

pub fn validate_task(task: &Task) -> TaskVerdict {
    let mut verdict = TaskVerdict {
        task: task.id.clone(),
        problems: Vec::new(),
        mutants: 0,
        mutants_caught: 0,
        mutants_public_green: 0,
    };
    let public = task.public();
    let public_count = crate::task::count_tests(&public.public_tests);
    let has_public = public_count > 0;
    if public_count < MIN_PUBLIC_TESTS {
        verdict.problems.push(format!(
            "only {public_count} public tests (minimum {MIN_PUBLIC_TESTS})"
        ));
    }
    let hidden_count = crate::task::count_tests(task.hidden_tests());
    if hidden_count < MIN_HIDDEN_TESTS {
        verdict.problems.push(format!(
            "only {hidden_count} hidden tests (minimum {MIN_HIDDEN_TESTS})"
        ));
    }

    match fs::read_to_string(task.dir().join(REFERENCE_FILE)) {
        Err(_) => verdict.problems.push(format!("missing {REFERENCE_FILE}")),
        Ok(reference) => {
            if let Some(score) = score_public(&public, Some(&reference))
                && !score.all_passed
            {
                verdict.problems.push(format!(
                    "reference fails public ({}/{})",
                    score.passed, score.expected
                ));
            }
            // API surface: tests must compile against the reference as a
            // separate crate, i.e. through its public items only, or every
            // candidate that keeps helpers private fails to compile.
            for (label, tests) in [
                ("public", &public.public_tests),
                ("hidden", &task.hidden_tests().to_string()),
            ] {
                if crate::task::count_tests(tests) > 0
                    && compile_tests_against_library(
                        &reference,
                        &format!(
                            "#![allow(unused_imports, dead_code)]\nuse a2d_reference::*;\n{tests}"
                        ),
                        "a2d_reference",
                        Duration::from_secs(120),
                    )
                    .is_err()
                {
                    verdict.problems.push(format!(
                        "{label} tests need non-public reference items (fail against a separate crate)"
                    ));
                }
            }
            let hidden = score_hidden(task, Some(&reference));
            // A second run must agree: flaky references make every score noise.
            let again = score_hidden(task, Some(&reference));
            if hidden.all_passed && (!again.all_passed || again.passed != hidden.passed) {
                verdict
                    .problems
                    .push("reference hidden results are not deterministic".to_string());
            }
            if !hidden.all_passed {
                verdict.problems.push(format!(
                    "reference fails hidden ({}/{}{})",
                    hidden.passed,
                    hidden.expected,
                    if hidden.compiled {
                        ""
                    } else {
                        ", does not compile"
                    }
                ));
            }
        }
    }

    let mut mutants: Vec<_> = fs::read_dir(task.dir().join(MUTANTS_DIR))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
                .collect()
        })
        .unwrap_or_default();
    mutants.sort();
    verdict.mutants = mutants.len();
    if mutants.len() < MIN_MUTANTS {
        verdict
            .problems
            .push(format!("{} mutants (minimum {MIN_MUTANTS})", mutants.len()));
    }
    for path in &mutants {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let Ok(code) = fs::read_to_string(path) else {
            verdict.problems.push(format!("unreadable mutant {stem}"));
            continue;
        };
        // Caught means: compiles against the hidden tests, runs to completion,
        // and at least one hidden assertion fails. Compile errors and
        // timeouts are not realistic bugs being caught.
        let hidden = score_hidden(task, Some(&code));
        if hidden.all_passed {
            verdict
                .problems
                .push(format!("mutant {stem} passes hidden"));
        } else if !hidden.compiled {
            verdict.problems.push(format!(
                "mutant {stem} does not compile against hidden tests"
            ));
        } else if hidden.timed_out {
            verdict
                .problems
                .push(format!("mutant {stem} is caught only by a timeout"));
        } else if hidden.failed == 0 {
            verdict
                .problems
                .push(format!("mutant {stem} is not caught by a failing test"));
        } else {
            verdict.mutants_caught += 1;
        }
        if score_public(&public, Some(&code)).is_some_and(|s| s.all_passed) {
            verdict.mutants_public_green += 1;
        }
    }
    if has_public && verdict.mutants_public_green < MIN_PUBLIC_GREEN_MUTANTS {
        verdict.problems.push(format!(
            "{} mutants pass public tests (minimum {MIN_PUBLIC_GREEN_MUTANTS}: hidden tests must add discrimination)",
            verdict.mutants_public_green
        ));
    }

    let uncovered = uncovered_api(&public.requirements, &public.public_tests);
    if has_public && !uncovered.is_empty() {
        verdict.problems.push(format!(
            "public tests never call {}",
            uncovered.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    let shared = test_names(&public.public_tests)
        .intersection(&test_names(task.hidden_tests()))
        .count();
    if shared > 0 {
        verdict
            .problems
            .push(format!("hidden repeats {shared} public test name(s)"));
    }
    verdict
}

/// `pub fn` names in the requirements that the public tests never call
/// (`name(` or `name::<`).
fn uncovered_api(requirements: &str, public_tests: &str) -> BTreeSet<String> {
    requirements
        .match_indices("pub fn ")
        .filter_map(|(at, _)| {
            let rest = &requirements[at + "pub fn ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .filter(|name| {
            !public_tests.contains(&format!("{name}("))
                && !public_tests.contains(&format!("{name}::<"))
        })
        .collect()
}

/// Names of `#[test]` functions in a test source file.
pub(crate) fn test_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut armed = false;
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#[test]")
            && let Some(rest) = rest.trim_start().strip_prefix("fn ")
        {
            if let Some(name) = rest.split(['(', '<', ' ']).next() {
                names.insert(name.to_string());
            }
            continue;
        }
        if line == "#[test]" {
            armed = true;
        } else if armed && let Some(rest) = line.strip_prefix("fn ") {
            if let Some(name) = rest.split(['(', '<', ' ']).next() {
                names.insert(name.to_string());
            }
            armed = false;
        } else if armed && !line.starts_with("#[") && !line.is_empty() {
            armed = false;
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE: &str =
        "pub fn clamp_add(a: u8, b: u8) -> u8 { a.saturating_add(b) }\nfn main() {}\n";
    const WRAPPING: &str =
        "pub fn clamp_add(a: u8, b: u8) -> u8 { a.wrapping_add(b) }\nfn main() {}\n";
    const PUBLIC: &str = "#[test]\nfn adds_small() { assert_eq!(clamp_add(1, 2), 3); }\n#[test]\nfn adds_zero() { assert_eq!(clamp_add(0, 0), 0); }\n#[test] fn adds_ten() { assert_eq!(clamp_add(7, 3), 10); }\n";
    const CLAMPED: &str =
        "pub fn clamp_add(a: u8, b: u8) -> u8 { a.saturating_add(b).min(254) }\nfn main() {}\n";
    const SUBTRACTS: &str =
        "pub fn clamp_add(a: u8, b: u8) -> u8 { a.saturating_sub(b) }\nfn main() {}\n";
    const ZERO: &str = "pub fn clamp_add(_a: u8, _b: u8) -> u8 { 0 }\nfn main() {}\n";

    fn hidden() -> String {
        let mut tests =
            String::from("#[test]\nfn saturates() { assert_eq!(clamp_add(250, 10), 255); }\n");
        for i in 0..19u8 {
            tests.push_str(&format!(
                "#[test]\nfn adds_{i}() {{ assert_eq!(clamp_add({i}, 3), {i} + 3); }}\n"
            ));
        }
        tests
    }

    fn mutants() -> Vec<(&'static str, &'static str)> {
        vec![
            ("wrapping", WRAPPING),
            ("clamped", CLAMPED),
            ("subtracts", SUBTRACTS),
            ("zero", ZERO),
        ]
    }

    fn write_task(root: &Path, mutants: &[(&str, &str)], hidden: &str) {
        let dir = root.join("clamp");
        fs::create_dir_all(dir.join(MUTANTS_DIR)).unwrap();
        fs::write(
            dir.join("task.json"),
            r#"{"id":"clamp","family":"arith","requirements":"Saturating add."}"#,
        )
        .unwrap();
        fs::write(dir.join("public_tests.rs"), PUBLIC).unwrap();
        fs::write(dir.join("hidden_tests.rs"), hidden).unwrap();
        fs::write(dir.join(REFERENCE_FILE), REFERENCE).unwrap();
        for (name, code) in mutants {
            fs::write(dir.join(MUTANTS_DIR).join(format!("{name}.rs")), code).unwrap();
        }
    }

    #[test]
    fn accepts_task_whose_hidden_tests_catch_a_public_passing_mutant() {
        let root = tempfile::tempdir().unwrap();
        write_task(root.path(), &mutants(), &hidden());
        let report = validate_suite(root.path(), 2).unwrap();
        let verdict = &report.verdicts[0];
        assert!(verdict.accepted(), "{verdict}");
        assert_eq!(
            (verdict.mutants_caught, verdict.mutants_public_green),
            (4, 2)
        );
        assert!(
            !verdict.to_string().contains("saturates"),
            "verdict leaks test names"
        );
    }

    #[test]
    fn rejects_oracle_that_passes_a_mutant_or_repeats_public_tests() {
        let root = tempfile::tempdir().unwrap();
        let weak_hidden = "#[test]\nfn adds_small() { assert_eq!(clamp_add(2, 2), 4); }\n";
        write_task(root.path(), &[("wrapping", WRAPPING)], weak_hidden);
        let verdict = &validate_suite(root.path(), 1).unwrap().verdicts[0];
        assert!(!verdict.accepted());
        let text = verdict.to_string();
        assert!(text.contains("mutant wrapping passes hidden"), "{text}");
        assert!(text.contains("repeats 1 public test name"), "{text}");
    }

    #[test]
    fn uncovered_api_lists_functions_public_tests_never_call() {
        let requirements =
            "API: `pub fn new() -> Self`, `pub fn get(&self, k: &str)`, `pub fn solve(&self)`.";
        let tests = "#[test]\nfn t() { let s = Store::new(); s.get(\"a\"); }\n";
        let uncovered = uncovered_api(requirements, tests);
        assert_eq!(uncovered.into_iter().collect::<Vec<_>>(), ["solve"]);
    }

    #[test]
    fn rejects_uncompilable_mutants_and_thin_oracles() {
        let root = tempfile::tempdir().unwrap();
        write_task(
            root.path(),
            &[("wrapping", WRAPPING), ("broken", "fn clamp_add(")],
            "#[test]\nfn saturates() { assert_eq!(clamp_add(250, 10), 255); }\n",
        );
        let text = validate_suite(root.path(), 1).unwrap().verdicts[0].to_string();
        assert!(text.contains("mutant broken does not compile"), "{text}");
        assert!(text.contains("2 mutants (minimum 4)"), "{text}");
        assert!(text.contains("only 1 hidden tests"), "{text}");
        assert!(text.contains("1 mutants pass public tests"), "{text}");
    }

    #[test]
    fn rejects_tests_that_need_private_reference_items() {
        let root = tempfile::tempdir().unwrap();
        write_task(root.path(), &mutants(), &hidden());
        // Reference keeps the function private: compiles under the oracle's
        // child module, but not through a separate crate.
        fs::write(
            root.path().join("clamp").join(REFERENCE_FILE),
            REFERENCE.replace("pub fn", "fn"),
        )
        .unwrap();
        let text = validate_suite(root.path(), 1).unwrap().verdicts[0].to_string();
        assert!(
            text.contains("hidden tests need non-public reference items"),
            "{text}"
        );
        assert!(
            text.contains("public tests need non-public reference items"),
            "{text}"
        );
    }

    #[test]
    fn test_names_reads_only_test_functions() {
        assert!(test_names("#[test] fn one_line() {}\n").contains("one_line"));
        let names = test_names(
            "fn helper() {}\n#[test]\n#[should_panic]\nfn panics() {}\n#[test]\nfn plain() {}\n",
        );
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["panics", "plain"]);
    }
}
