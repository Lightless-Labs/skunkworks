//! Baseline arms A (one-shot), B (best-of-k) and D (repair loop), and the
//! frozen submission rule shared by B, C and D.

use crate::oracle::{TestScore, score_public};
use crate::task::PublicTask;
use a2d_core::benchmark::extract_rust_code_pub;
use a2d_core::provider::{InvocationRequest, Provider};
use a2d_core::types::EnzymeId;

pub const CODER_ENZYME: &str = "lift-coder";

pub const CODER_SYSTEM: &str = "You are an expert Rust engineer. Write one complete, \
self-contained Rust source file that satisfies the task, using only the standard library. \
Respond with the whole file in a single ```rust fenced code block and nothing else.";

const MAX_FEEDBACK_CHARS: usize = 6_000;

/// One coder attempt and its public score (`None` when the task has no public tests).
#[derive(Debug, Clone)]
pub struct Attempt {
    pub index: usize,
    pub code: Option<String>,
    pub public: Option<TestScore>,
}

impl Attempt {
    fn public_passed(&self) -> usize {
        self.public.as_ref().map(|s| s.passed).unwrap_or(0)
    }

    fn public_green(&self) -> bool {
        self.public.as_ref().is_some_and(|s| s.all_passed)
    }
}

pub fn task_prompt(task: &PublicTask) -> String {
    let mut prompt = task.requirements.trim_end().to_string();
    if !task.public_tests.trim().is_empty() {
        prompt.push_str(
            "\n\n## Public tests\n\nYour file must pass these tests. They are appended to your \
             file inside `#[cfg(test)] mod <name> { use super::*; ... }`. Additional hidden tests \
             check the same requirements.\n\n```rust\n",
        );
        prompt.push_str(task.public_tests.trim_end());
        prompt.push_str("\n```\n");
    }
    prompt
}

fn attempt(provider: &dyn Provider, task: &PublicTask, index: usize, prompt: String) -> Attempt {
    let request = InvocationRequest {
        enzyme_id: EnzymeId::from(CODER_ENZYME),
        system: CODER_SYSTEM.to_string(),
        prompt,
        max_tokens: 32_000,
    };
    let code = provider
        .invoke(&request)
        .ok()
        .and_then(|response| extract_rust_code_pub(&response.text));
    let public = score_public(task, code.as_deref());
    Attempt {
        index,
        code,
        public,
    }
}

/// Arm A: pass@1.
pub fn one_shot(provider: &dyn Provider, task: &PublicTask) -> Vec<Attempt> {
    vec![attempt(provider, task, 0, task_prompt(task))]
}

/// Arm B: k independent attempts on the same prompt.
pub fn best_of_k(provider: &dyn Provider, task: &PublicTask, k: usize) -> Vec<Attempt> {
    (0..k)
        .map(|index| attempt(provider, task, index, task_prompt(task)))
        .collect()
}

/// Arm D: up to k attempts; each retry sees the previous file and its public
/// test output. Stops early once public tests are green.
pub fn repair_loop(provider: &dyn Provider, task: &PublicTask, k: usize) -> Vec<Attempt> {
    let mut attempts: Vec<Attempt> = Vec::new();
    for index in 0..k {
        let prompt = match attempts.last() {
            None => task_prompt(task),
            Some(previous) => repair_prompt(task, previous),
        };
        let current = attempt(provider, task, index, prompt);
        let green = current.public_green();
        attempts.push(current);
        if green {
            break;
        }
    }
    attempts
}

fn repair_prompt(task: &PublicTask, previous: &Attempt) -> String {
    let mut prompt = task_prompt(task);
    match &previous.code {
        None => prompt.push_str(
            "\n\n## Previous attempt\n\nYour previous response contained no ```rust code block.\n",
        ),
        Some(code) => {
            prompt.push_str("\n\n## Previous attempt\n\n```rust\n");
            prompt.push_str(code.trim_end());
            prompt.push_str("\n```\n\n## Public test results\n\n");
            let feedback = previous
                .public
                .as_ref()
                .map(|s| s.output.as_str())
                .unwrap_or("(this task has no public tests)");
            prompt.push_str(&truncate(feedback, MAX_FEEDBACK_CHARS));
        }
    }
    prompt.push_str(
        "\n\nFix the file. Respond with the whole corrected file in a single ```rust block.\n",
    );
    prompt
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... [truncated]", &text[..end])
}

/// Frozen submission rule: the attempt with code that passes the most public
/// tests; ties go to the earliest attempt. `None` if no attempt produced code.
pub fn select_submission(attempts: &[Attempt]) -> Option<&Attempt> {
    attempts.iter().filter(|a| a.code.is_some()).fold(
        None,
        |best: Option<&Attempt>, a| match best {
            Some(b) if b.public_passed() >= a.public_passed() => Some(b),
            _ => Some(a),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(index: usize, code: bool, passed: usize) -> Attempt {
        Attempt {
            index,
            code: code.then(|| format!("// {index}")),
            public: Some(TestScore {
                compiled: code,
                passed,
                failed: 3 - passed,
                expected: 3,
                timed_out: false,
                all_passed: passed == 3,
                output: String::new(),
            }),
        }
    }

    #[test]
    fn selection_prefers_most_public_passes_then_earliest() {
        let attempts = [
            att(0, true, 1),
            att(1, true, 2),
            att(2, true, 2),
            att(3, false, 0),
        ];
        assert_eq!(select_submission(&attempts).unwrap().index, 1);
        let ties = [att(0, true, 0), att(1, true, 0)];
        assert_eq!(select_submission(&ties).unwrap().index, 0);
        let none = [att(0, false, 0)];
        assert!(select_submission(&none).is_none());
    }

    #[test]
    fn prompt_includes_public_tests_only_when_present() {
        let mut task = PublicTask {
            id: "t".into(),
            family: "f".into(),
            requirements: "Do it.".into(),
            public_tests: String::new(),
            test_timeout_secs: 5,
        };
        assert_eq!(task_prompt(&task), "Do it.");
        task.public_tests = "#[test]\nfn visible() {}\n".into();
        assert!(task_prompt(&task).contains("fn visible()"));
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "é".repeat(10);
        let cut = truncate(&text, 5);
        assert!(cut.starts_with("éé"));
        assert!(cut.ends_with("[truncated]"));
    }
}
