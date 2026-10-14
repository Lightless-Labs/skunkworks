//! Oracle validation (`a2d lift validate`): a task is accepted only if its
//! independent reference passes every public and hidden test, every mutant
//! fails at least one hidden test, at least one mutant passes the public
//! tests (so hidden tests discriminate beyond them), and hidden tests do not
//! repeat public test names. Verdicts carry counts and file stems only,
//! never test content, so holdout sets can be validated blind.

use crate::oracle::{score_hidden, score_public};
use crate::task::{Task, load_suite};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const REFERENCE_FILE: &str = "reference.rs";
pub const MUTANTS_DIR: &str = "mutants";

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
    let has_public = !public.public_tests.trim().is_empty();

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
            let hidden = score_hidden(task, Some(&reference));
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
    if mutants.is_empty() {
        verdict.problems.push("no mutants".to_string());
    }
    for path in &mutants {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let Ok(code) = fs::read_to_string(path) else {
            verdict.problems.push(format!("unreadable mutant {stem}"));
            continue;
        };
        if score_hidden(task, Some(&code)).all_passed {
            verdict
                .problems
                .push(format!("mutant {stem} passes hidden"));
        } else {
            verdict.mutants_caught += 1;
        }
        if score_public(&public, Some(&code)).is_some_and(|s| s.all_passed) {
            verdict.mutants_public_green += 1;
        }
    }
    if has_public && !mutants.is_empty() && verdict.mutants_public_green == 0 {
        verdict.problems.push(
            "no mutant passes public tests (hidden adds no shown discrimination)".to_string(),
        );
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

/// Names of `#[test]` functions in a test source file.
pub(crate) fn test_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut armed = false;
    for line in source.lines().map(str::trim) {
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
        "fn clamp_add(a: u8, b: u8) -> u8 { a.saturating_add(b) }\nfn main() {}\n";
    const WRAPPING: &str = "fn clamp_add(a: u8, b: u8) -> u8 { a.wrapping_add(b) }\nfn main() {}\n";
    const PUBLIC: &str = "#[test]\nfn adds_small() { assert_eq!(clamp_add(1, 2), 3); }\n";
    const HIDDEN: &str = "#[test]\nfn saturates() { assert_eq!(clamp_add(250, 10), 255); }\n";

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
        write_task(root.path(), &[("wrapping", WRAPPING)], HIDDEN);
        let report = validate_suite(root.path(), 2).unwrap();
        let verdict = &report.verdicts[0];
        assert!(verdict.accepted(), "{verdict}");
        assert_eq!(
            (verdict.mutants_caught, verdict.mutants_public_green),
            (1, 1)
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
    fn test_names_reads_only_test_functions() {
        let names = test_names(
            "fn helper() {}\n#[test]\n#[should_panic]\nfn panics() {}\n#[test]\nfn plain() {}\n",
        );
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["panics", "plain"]);
    }
}
