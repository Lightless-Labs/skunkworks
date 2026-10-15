//! Baseline arms A (one-shot), B (best-of-k) and D (repair loop), and the
//! frozen submission rule shared by B, C and D.

use crate::meter::{MeteredProvider, Scope, SharedMeter, lock};
use crate::oracle::{TestScore, score_public};
use crate::task::PublicTask;
use a2d_core::benchmark::extract_rust_code_pub;
use a2d_core::provider::{InvocationRequest, Provider};
use a2d_core::types::EnzymeId;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

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
    /// Selection key: public tests passed, then whether it compiled.
    fn public_rank(&self) -> (usize, bool) {
        self.public
            .as_ref()
            .map(|s| (s.passed, s.compiled))
            .unwrap_or((0, false))
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
    repair_loop_with_note(provider, task, k, None)
}

pub fn repair_loop_with_note(
    provider: &dyn Provider,
    task: &PublicTask,
    k: usize,
    note: Option<&str>,
) -> Vec<Attempt> {
    let mut base = task_prompt(task);
    if let Some(note) = note {
        base.push_str("\n\n");
        base.push_str(note);
    }
    let mut attempts: Vec<Attempt> = Vec::new();
    for index in 0..k {
        let prompt = match attempts.last() {
            None => base.clone(),
            Some(previous) => repair_prompt(previous, base.clone()),
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

/// Shared scoring for code artifacts supplied by C and external harnesses.
pub(crate) fn score_codes(task: &PublicTask, codes: Vec<String>) -> Vec<Attempt> {
    codes
        .into_iter()
        .enumerate()
        .map(|(index, code)| Attempt {
            index,
            public: score_public(task, Some(&code)),
            code: Some(code),
        })
        .collect()
}

/// The real Python interpreter, resolved once under the controller's own
/// environment. Apple's `/usr/bin/python3` is an `xcrun` shim; with a cleared
/// environment it can spend tens of seconds re-resolving the toolchain before
/// the harness starts, so children are launched with the resolved path.
fn python_interpreter() -> std::path::PathBuf {
    static PYTHON: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PYTHON
        .get_or_init(|| {
            Command::new("python3")
                .args(["-I", "-c", "import sys; print(sys.executable)"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "python3".into())
        })
        .clone()
}

/// Run a Python harness with only public data and metered controller operations.
/// Blocking pipe I/O runs on a worker so even a silent or non-reading child
/// can be killed at the controller deadline. Stderr goes directly to a file,
/// avoiding a full stderr pipe blocking the episode.
pub(crate) fn external_harness(
    path: &Path,
    provider: MeteredProvider,
    lanes: std::collections::BTreeMap<String, crate::Lane>,
    task: &PublicTask,
    k: usize,
    meter: &SharedMeter,
    scope: &Arc<Scope>,
) -> Vec<String> {
    let episode = || -> Result<Vec<String>, String> {
        let path = fs::canonicalize(path).map_err(|e| format!("external path: {e}"))?;
        let cwd = tempfile::Builder::new()
            .prefix("a2d-external-")
            .tempdir()
            .map_err(|e| format!("external cwd: {e}"))?;
        let dir = lock(meter).invocation_dir(scope);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stderr = fs::File::create(dir.join("harness.stderr.txt")).map_err(|e| e.to_string())?;
        let mut command = Command::new(python_interpreter());
        command
            .arg("-I")
            .arg(path)
            .current_dir(cwd.path())
            .env_clear();
        for name in ["PATH", "HOME", "LANG", "TMPDIR"] {
            let fallback = if name == "LANG" { "C.UTF-8" } else { "" };
            command.env(
                name,
                std::env::var_os(name).unwrap_or_else(|| fallback.into()),
            );
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .map_err(|e| format!("start external: {e}"))?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let task = task.clone();
        let worker_meter = meter.clone();
        let lane_meter = meter.clone();
        let lane_scope = scope.clone();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<Vec<String>, String> {
                let mut reply = |value: &Value| -> Result<(), String> {
                    writeln!(stdin, "{value}")
                        .and_then(|_| stdin.flush())
                        .map_err(|e| format!("external stdin: {e}"))
                };
                reply(&json!({"task": {
                    "id": task.id, "family": task.family,
                    "task_prompt": task_prompt(&task), "coder_system": CODER_SYSTEM,
                    "k": k, "max_feedback_chars": MAX_FEEDBACK_CHARS,
                }}))?;
                for line in BufReader::new(stdout).lines() {
                    if lock(&worker_meter).past_deadline() {
                        return Err("wall-clock cap reached".to_string());
                    }
                    let request: Value = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                        .map_err(|e| format!("external JSON: {e}"))?;
                    let string = |key: &str| {
                        request[key]
                            .as_str()
                            .ok_or_else(|| format!("{key} must be a string"))
                    };
                    let response = (|| -> Result<Value, String> {
                        match request["op"].as_str() {
                            Some("model") => {
                                let lane = request["lane"].as_str();
                                let request = InvocationRequest {
                                    enzyme_id: EnzymeId::from(
                                        lane.map_or(CODER_ENZYME.to_string(), |l| format!("lane:{l}"))
                                            .as_str(),
                                    ),
                                    system: string("system")?.to_string(),
                                    prompt: string("prompt")?.to_string(),
                                    max_tokens: 32_000,
                                };
                                match lane {
                                    None => provider.invoke(&request),
                                    Some(name) => match lanes.get(name) {
                                        // Same meter and scope: lane calls share the budget k.
                                        Some(lane) => MeteredProvider::new(
                                            lane.provider.clone(),
                                            lane_meter.clone(),
                                            lane_scope.clone(),
                                        )
                                        .invoke(&request),
                                        None => return Err(format!("unknown lane {name:?}")),
                                    },
                                }
                                .map(|r| json!({"ok": true, "text": r.text}))
                                .map_err(|e| e.to_string())
                            }
                            Some("extract") => {
                                Ok(json!({"code": extract_rust_code_pub(string("text")?)}))
                            }
                            Some("public_test") => {
                                // TestScore omits output in the hidden ledger, but public
                                // feedback must include it for the harness's repair loop.
                                Ok(match score_public(&task, Some(string("code")?)) {
                                    Some(score) => {
                                        let mut value = serde_json::to_value(&score)
                                            .map_err(|e| e.to_string())?;
                                        value["output"] = json!(score.output);
                                        value
                                    }
                                    None => Value::Null,
                                })
                            }
                            Some("done") => Ok(json!({"ok": true})),
                            _ => Err("unknown op".to_string()),
                        }
                    })()
                    .unwrap_or_else(|error| json!({"ok": false, "error": error}));
                    if request["op"] == "done" {
                        let codes = serde_json::from_value(request["codes"].clone())
                            .map_err(|e| format!("done codes: {e}"))?;
                        // Receiving `done` commits the codes even if the child
                        // has already closed stdin while exiting.
                        let _ = reply(&response);
                        return Ok(codes);
                    }
                    reply(&response)?;
                }
                Err("external exited without done".to_string())
            })();
            let _ = send.send(result);
        });
        let result = loop {
            if lock(meter).past_deadline() {
                scope.mark_void("wall-clock cap reached");
                break Err("wall-clock cap reached".to_string());
            }
            match receive.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("external worker stopped".to_string());
                }
            }
        };
        // `done` ends the episode even if the harness stays alive afterward.
        // Do not join the I/O worker: a grandchild may still hold its pipe.
        let _ = child.kill();
        let _ = child.wait();
        if lock(meter).past_deadline() {
            scope.mark_void("wall-clock cap reached");
            Err("wall-clock cap reached".to_string())
        } else {
            result
        }
    };
    episode().unwrap_or_else(|error| {
        let _ = lock(meter).ledger.append(
            "external_error",
            json!({
                "arm": scope.arm, "task": scope.task, "replicate": scope.replicate, "error": error,
            }),
        );
        Vec::new()
    })
}

fn repair_prompt(previous: &Attempt, mut prompt: String) -> String {
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

/// Frozen submission rule: among attempts with code, the most public tests
/// passed, then compiled over not compiled, then the **latest** attempt.
/// For the feedback arms (D, C) a later attempt with an equal public score
/// has absorbed more feedback, and A²D itself keeps its latest code; for B
/// the attempts are exchangeable, so the tie order does not change its
/// distribution. (Changed from "earliest" at the P1 gate, before any
/// attempt-level hidden results were examined.) `None` if no attempt
/// produced code.
pub fn select_submission(attempts: &[Attempt]) -> Option<&Attempt> {
    attempts.iter().filter(|a| a.code.is_some()).fold(
        None,
        |best: Option<&Attempt>, a| match best {
            Some(b) if b.public_rank() > a.public_rank() => Some(b),
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
                exit_success: passed == 3,
                all_passed: passed == 3,
                output: String::new(),
            }),
        }
    }

    #[test]
    fn selection_prefers_most_public_passes_then_latest() {
        let attempts = [
            att(0, true, 1),
            att(1, true, 2),
            att(2, true, 2),
            att(3, false, 0),
        ];
        assert_eq!(select_submission(&attempts).unwrap().index, 2);
        let ties = [att(0, true, 0), att(1, true, 0)];
        assert_eq!(select_submission(&ties).unwrap().index, 1);
        let earlier_better = [att(0, true, 3), att(1, true, 2)];
        assert_eq!(select_submission(&earlier_better).unwrap().index, 0);
        let mut not_compiled = att(0, true, 0);
        not_compiled.public.as_mut().unwrap().compiled = false;
        let compiled = [not_compiled, att(1, true, 0)];
        assert_eq!(select_submission(&compiled).unwrap().index, 1);
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
