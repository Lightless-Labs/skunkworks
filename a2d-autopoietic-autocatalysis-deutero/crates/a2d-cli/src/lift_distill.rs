//! `a2d distill`: A²D's architect writes task-agnostic lessons from train
//! labels (P4).
//!
//! Inputs are development runs scored by `a2d lift run` and their suites. For
//! every submission of the chosen arm that passed all public tests but failed
//! hidden scoring, the architect gets a dossier with:
//! - what the coder saw (requirements and public tests);
//! - the submitted file;
//! - the label "fails at least one hidden test".
//!
//! Hidden test content and hidden output never enter a prompt (PROTOCOL
//! hidden-test boundary; P4 train-label rule). The architect names suspect
//! clauses and proposes one lesson per dossier. A final call merges the
//! proposals with the harness's current lessons into at most `max` lessons.
//! Lessons that name a train family or a distinctive train API identifier are
//! dropped.

use a2d_core::provider::{InvocationRequest, Provider};
use a2d_core::types::EnzymeId;
use a2d_lift::task::PublicTask;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ARCHITECT_SYSTEM: &str = "You are the architect of a coding harness. You study a programmer's failures on training tasks and write short, general lessons that help on future, different tasks.";

const DIAGNOSE: &str = "This Rust file passes every visible test, but it FAILS at least one hidden acceptance test that checks the requirements below.\nFind the bug. Reply with ONE JSON object and nothing else:\n{\"suspect_clauses\": [up to 3 clause ids such as \"R7\", most likely first], \"bug\": \"one sentence\", \"lesson\": \"a TASK-AGNOSTIC instruction to a programmer (no names from this task, at most 25 words) that would have prevented this bug\"}";

/// One public-green, hidden-fail submission, as the architect may see it.
#[derive(Debug, Clone)]
pub struct Dossier {
    pub task: String,
    pub family: String,
    /// What the coder saw: requirements plus public tests.
    pub task_prompt: String,
    pub code: String,
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The public part of a task, read field by field: `hidden_tests.rs` is
/// never opened.
fn public_task(suite: &Path, task: &str) -> Result<PublicTask, String> {
    let dir = suite.join(task);
    let meta: Value = serde_json::from_str(&read(&dir.join("task.json"))?)
        .map_err(|e| format!("{task}: task.json: {e}"))?;
    Ok(PublicTask {
        id: task.to_string(),
        family: meta["family"].as_str().unwrap_or_default().to_string(),
        requirements: meta["requirements"].as_str().unwrap_or_default().to_string(),
        public_tests: fs::read_to_string(dir.join("public_tests.rs")).unwrap_or_default(),
        test_timeout_secs: meta["test_timeout_secs"].as_u64().unwrap_or(10),
    })
}

/// Public-green, hidden-fail submissions of `arm` in a scored run.
pub fn failure_dossiers(run_dir: &Path, suite: &Path, arm: &str) -> Result<Vec<Dossier>, String> {
    let mut green = BTreeMap::new();
    let mut hidden_fail = BTreeSet::new();
    for line in read(&run_dir.join("ledger.jsonl"))?.lines() {
        let record: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if record["arm"].as_str() != Some(arm) {
            continue;
        }
        let task = record["task"].as_str().unwrap_or_default().to_string();
        let replicate = record["replicate"].as_u64().unwrap_or(0);
        match record["kind"].as_str() {
            Some("submission") if record["public"]["all_passed"].as_bool() == Some(true) => {
                let family = record["family"].as_str().unwrap_or_default().to_string();
                green.insert((task, replicate), family);
            }
            Some("score") if record["result"]["all_passed"].as_bool() == Some(false) => {
                hidden_fail.insert((task, replicate));
            }
            _ => {}
        }
    }
    let mut dossiers = Vec::new();
    for ((task, replicate), family) in green {
        if !hidden_fail.contains(&(task.clone(), replicate)) {
            continue;
        }
        let submission: PathBuf = run_dir
            .join("submissions")
            .join(arm)
            .join(&task)
            .join(format!("r{replicate}.rs"));
        dossiers.push(Dossier {
            task_prompt: a2d_lift::arms::task_prompt(&public_task(suite, &task)?),
            code: read(&submission)?,
            task,
            family,
        });
    }
    Ok(dossiers)
}

const UNINFORMED: &str = "Below are the requirements of a Rust programming task.\nWrite one TASK-AGNOSTIC instruction to a programmer (no names from this task, at most 25 words) that helps implement tasks like this correctly. Reply with ONE JSON object and nothing else:\n{\"suspect_clauses\": [], \"bug\": \"\", \"lesson\": \"...\"}";

/// Deterministic sampling order for the uninformed control.
pub fn sample_key(seed: u64, task: &str, replicate: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for byte in task.bytes().chain(replicate.to_le_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Control: `n` submissions of `arm`, sampled without regard to any outcome.
/// The architect sees requirements only (no file, no label).
pub fn uninformed_dossiers(
    run_dir: &Path,
    suite: &Path,
    arm: &str,
    n: usize,
    seed: u64,
) -> Result<Vec<Dossier>, String> {
    let mut picks = Vec::new();
    for line in read(&run_dir.join("ledger.jsonl"))?.lines() {
        let record: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if record["arm"].as_str() == Some(arm) && record["kind"].as_str() == Some("submission") {
            let task = record["task"].as_str().unwrap_or_default().to_string();
            let replicate = record["replicate"].as_u64().unwrap_or(0);
            let family = record["family"].as_str().unwrap_or_default().to_string();
            picks.push((sample_key(seed, &task, replicate), task, family));
        }
    }
    picks.sort();
    picks
        .into_iter()
        .take(n)
        .map(|(_, task, family)| {
            Ok(Dossier {
                task_prompt: a2d_lift::arms::task_prompt(&public_task(suite, &task)?),
                code: String::new(),
                task,
                family,
            })
        })
        .collect()
}

pub fn uninformed_prompt(dossier: &Dossier) -> String {
    format!("{UNINFORMED}\n\nREQUIREMENTS:\n{}", dossier.task_prompt)
}

pub fn diagnosis_prompt(dossier: &Dossier) -> String {
    format!(
        "{DIAGNOSE}\n\nREQUIREMENTS:\n{}\n\nFILE:\n```rust\n{}\n```",
        dossier.task_prompt, dossier.code
    )
}

pub fn merge_prompt(current: &[String], proposed: &[String], max: usize) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "(none)".to_string()
        } else {
            items
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}. {l}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    format!(
        "You maintain a short list of lessons for a programmer who solves many DIFFERENT Rust tasks from numbered requirements.\n\nCURRENT LESSONS:\n{}\n\nNEW LESSONS proposed from recent failures, one per failed task (repeats mean frequent mistakes):\n{}\n\nWrite the revised list: at most {max} lessons; each task-agnostic (no names of tasks, types or functions), imperative, at most 25 words; most frequent mistakes first. Merge duplicates. Keep a current lesson unless a better general lesson covers it.\nReply with ONE JSON object and nothing else: {{\"lessons\": [\"...\"]}}",
        list(current),
        list(proposed)
    )
}

fn first_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// Identifiers distinctive enough that a lesson naming them is task-specific:
/// family names, and snake_case or CamelCase API item names.
pub fn train_identifiers(dossiers: &[Dossier]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for d in dossiers {
        ids.insert(d.family.clone());
        let words: Vec<&str> = d
            .task_prompt
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .collect();
        for pair in words.windows(2) {
            if matches!(pair[0], "fn" | "struct" | "enum" | "trait" | "type") {
                let name = pair[1];
                let camel = name.chars().skip(1).any(|c| c.is_ascii_uppercase());
                if name.contains('_') || camel {
                    ids.insert(name.to_string());
                }
            }
        }
    }
    ids.retain(|id| !id.is_empty());
    ids
}

fn names_any(lesson: &str, ids: &BTreeSet<String>) -> bool {
    let words: BTreeSet<&str> = lesson
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .collect();
    ids.iter().any(|id| words.contains(id.as_str()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnosis {
    pub task: String,
    pub suspect_clauses: Vec<String>,
    pub bug: String,
    pub lesson: String,
}

pub struct Distilled {
    pub lessons: Vec<String>,
    pub diagnoses: Vec<Diagnosis>,
    pub dropped: Vec<String>,
    pub calls: usize,
}

fn invoke(provider: &dyn Provider, enzyme: &str, prompt: String) -> String {
    let request = InvocationRequest {
        enzyme_id: EnzymeId::from(enzyme),
        system: ARCHITECT_SYSTEM.to_string(),
        prompt,
        max_tokens: 4096,
    };
    match provider.invoke(&request) {
        Ok(response) => response.text,
        Err(error) => {
            eprintln!("distill: {enzyme} call failed: {error}");
            String::new()
        }
    }
}

/// One diagnosis call per dossier (`jobs` at a time), then one merge call.
pub fn distill(
    provider: Arc<dyn Provider>,
    dossiers: &[Dossier],
    current: &[String],
    max: usize,
    jobs: usize,
) -> Distilled {
    let mut diagnoses = Vec::new();
    for chunk in dossiers.chunks(jobs.max(1)) {
        let answers: Vec<(String, String)> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|d| {
                    let provider = provider.clone();
                    scope.spawn(move || {
                        // An uninformed (control) dossier has no file.
                        let prompt = if d.code.is_empty() {
                            uninformed_prompt(d)
                        } else {
                            diagnosis_prompt(d)
                        };
                        (
                            d.task.clone(),
                            invoke(provider.as_ref(), "architect-diagnose", prompt),
                        )
                    })
                })
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        for (task, text) in answers {
            let Some(value) = first_json(&text) else {
                continue;
            };
            diagnoses.push(Diagnosis {
                task,
                suspect_clauses: value["suspect_clauses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect(),
                bug: value["bug"].as_str().unwrap_or_default().to_string(),
                lesson: value["lesson"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
    let ids = train_identifiers(dossiers);
    let proposed: Vec<String> = diagnoses
        .iter()
        .map(|d| d.lesson.clone())
        .filter(|l| !l.trim().is_empty() && !names_any(l, &ids))
        .collect();
    let merged = invoke(
        provider.as_ref(),
        "architect-merge",
        merge_prompt(current, &proposed, max),
    );
    let mut lessons = Vec::new();
    let mut dropped = Vec::new();
    for lesson in first_json(&merged)
        .and_then(|v| v["lessons"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| l.as_str().map(|s| s.trim().to_string()))
        .filter(|l| !l.is_empty())
    {
        if names_any(&lesson, &ids)
            || lesson.chars().count() > crate::lift_arm_c::MAX_LESSON_CHARS
        {
            dropped.push(lesson);
        } else if lessons.len() < max {
            lessons.push(lesson);
        }
    }
    if lessons.is_empty() {
        // A failed merge keeps the incumbent lessons rather than erasing them.
        lessons = current.to_vec();
    }
    Distilled {
        lessons,
        diagnoses,
        dropped,
        calls: dossiers.len() + 1,
    }
}

const USAGE: &str = "usage: a2d distill --from <run-dir>=<suite-dir> [--from ...] --harness <in.json> --out <out.json> --log <provenance.json> --name <name> [--arm C] [--model codex/gpt-6-luna] [--effort low] [--max-lessons 8] [--jobs 4] [--mode informed|uninformed] [--seed <n>]";

pub fn main(args: &[String]) -> i32 {
    match run(args) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            2
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut from = Vec::new();
    let mut flags = BTreeMap::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--from" => {
                let (run, suite) = value.split_once('=').ok_or("--from <run-dir>=<suite-dir>")?;
                from.push((PathBuf::from(run), PathBuf::from(suite)));
            }
            f if f.starts_with("--") => {
                flags.insert(f.trim_start_matches("--").to_string(), value.clone());
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    let need = |key: &str| flags.get(key).cloned().ok_or(format!("--{key} is required"));
    if from.is_empty() {
        return Err("at least one --from is required".to_string());
    }
    let arm = flags.get("arm").cloned().unwrap_or_else(|| "C".to_string());
    let max: usize = flags
        .get("max-lessons")
        .map(|v| v.parse().map_err(|_| "--max-lessons must be a number"))
        .transpose()?
        .unwrap_or(8)
        .min(crate::lift_arm_c::MAX_LESSONS);
    let jobs: usize = flags.get("jobs").and_then(|v| v.parse().ok()).unwrap_or(4);
    let mut harness: Value =
        serde_json::from_str(&read(Path::new(&need("harness")?))?).map_err(|e| e.to_string())?;
    let current: Vec<String> = harness["lessons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l.as_str().map(str::to_string))
        .collect();
    let mut dossiers = Vec::new();
    for (run_dir, suite) in &from {
        dossiers.extend(failure_dossiers(run_dir, suite, &arm)?);
    }
    let mode = flags.get("mode").cloned().unwrap_or_else(|| "informed".to_string());
    match mode.as_str() {
        "informed" => {}
        "uninformed" => {
            // Control at equal size: as many dossiers as there are failures,
            // sampled from every submission regardless of outcome.
            let seed: u64 = flags.get("seed").and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut pool = Vec::new();
            for (run_dir, suite) in &from {
                for d in uninformed_dossiers(run_dir, suite, &arm, usize::MAX, seed)? {
                    pool.push((sample_key(seed, &d.task, 0), d));
                }
            }
            pool.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.task.cmp(&b.1.task)));
            dossiers = pool.into_iter().take(dossiers.len()).map(|(_, d)| d).collect();
        }
        other => return Err(format!("--mode must be informed or uninformed, not {other:?}")),
    }
    let model = flags
        .get("model")
        .cloned()
        .unwrap_or_else(|| "codex/gpt-6-luna".to_string());
    let effort = flags.get("effort").cloned().unwrap_or_else(|| "low".to_string());
    // The same Codex lane as `a2d lift run --model` (the treatment model).
    let provider: Arc<dyn Provider> = match model.split_once('/') {
        Some(("codex", name)) if !name.is_empty() => Arc::new(
            a2d_providers::cli::CliProvider::codex(name, Some(&effort)),
        ),
        _ => return Err(format!("unsupported --model {model:?}; use codex/<model>")),
    };
    let distilled = distill(provider, &dossiers, &current, max, jobs);
    harness["name"] = json!(need("name")?);
    harness["lessons"] = json!(distilled.lessons);
    crate::lift_arm_c::validate(&harness)?;
    let text = serde_json::to_string_pretty(&harness).map_err(|e| e.to_string())?;
    fs::write(need("out")?, text + "\n").map_err(|e| e.to_string())?;
    let provenance = json!({
        "model": model, "effort": effort, "arm": arm, "mode": mode,
        "from": from.iter().map(|(r, s)| json!({"run": r, "suite": s})).collect::<Vec<_>>(),
        "dossiers": dossiers.iter().map(|d| &d.task).collect::<Vec<_>>(),
        "calls": distilled.calls,
        "diagnoses": distilled.diagnoses,
        "dropped": distilled.dropped,
        "previous_lessons": current,
        "lessons": distilled.lessons,
    });
    fs::write(
        need("log")?,
        serde_json::to_string_pretty(&provenance).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    println!(
        "distill: {} dossiers, {} calls, {} lessons ({} dropped)",
        dossiers.len(),
        distilled.calls,
        distilled.lessons.len(),
        distilled.dropped.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2d_core::provider::{InvocationResponse, ProviderError, TokenUsage};
    use std::sync::Mutex;

    const SENTINEL: &str = "HIDDEN_SENTINEL_d1st";

    struct Recorder {
        seen: Mutex<Vec<String>>,
        merge_reply: String,
    }

    impl Provider for Recorder {
        fn name(&self) -> &str {
            "mock/recorder"
        }

        fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{}\n{}", request.system, request.prompt));
            let text = if request.enzyme_id.0 == "architect-merge" {
                self.merge_reply.clone()
            } else {
                r#"{"suspect_clauses": ["R2"], "bug": "b", "lesson": "Validate in the specified order."}"#
                    .to_string()
            };
            Ok(InvocationResponse {
                text,
                raw_output: None,
                tool_events: Vec::new(),
                transcript: None,
                thinking: None,
                usage: TokenUsage::default(),
            })
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let suite = root.path().join("suite");
        for task in ["t1", "t2"] {
            let dir = suite.join(task);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("task.json"),
                format!(
                    r#"{{"id":"{task}","family":"fam-x","requirements":"R1. Implement `pub fn parse_thing(s: &str) -> u8`.\nR2. Errors first."}}"#
                ),
            )
            .unwrap();
            fs::write(dir.join("public_tests.rs"), "#[test]\nfn visible() {}\n").unwrap();
            fs::write(
                dir.join("hidden_tests.rs"),
                format!("#[test]\nfn {SENTINEL}() {{}}\n"),
            )
            .unwrap();
        }
        let run = root.path().join("run");
        for (arm, task) in [("C", "t1"), ("C", "t2"), ("D", "t1")] {
            let dir = run.join("submissions").join(arm).join(task);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("r0.rs"), format!("// CODE_{arm}_{task}\nfn main() {{}}\n")).unwrap();
        }
        let ledger = [
            json!({"kind": "submission", "arm": "C", "task": "t1", "family": "fam-x", "replicate": 0, "public": {"all_passed": true}}),
            json!({"kind": "submission", "arm": "C", "task": "t2", "family": "fam-x", "replicate": 0, "public": {"all_passed": true}}),
            json!({"kind": "submission", "arm": "D", "task": "t1", "family": "fam-x", "replicate": 0, "public": {"all_passed": true}}),
            json!({"kind": "score", "arm": "C", "task": "t1", "family": "fam-x", "replicate": 0, "result": {"all_passed": false, "failed": 3}}),
            json!({"kind": "score", "arm": "C", "task": "t2", "family": "fam-x", "replicate": 0, "result": {"all_passed": true}}),
            json!({"kind": "score", "arm": "D", "task": "t1", "family": "fam-x", "replicate": 0, "result": {"all_passed": false}}),
        ]
        .iter()
        .map(|r| r.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        fs::write(run.join("ledger.jsonl"), ledger + "\n").unwrap();
        (root, run, suite)
    }

    #[test]
    fn dossiers_are_public_green_hidden_fail_and_hidden_tests_never_reach_the_architect() {
        let (_root, run, suite) = fixture();
        let dossiers = failure_dossiers(&run, &suite, "C").unwrap();
        assert_eq!(dossiers.len(), 1);
        assert_eq!(dossiers[0].task, "t1");
        assert!(dossiers[0].code.contains("CODE_C_t1"));
        assert!(dossiers[0].task_prompt.contains("Errors first."));
        assert!(dossiers[0].task_prompt.contains("fn visible()"));
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
            merge_reply: r#"{"lessons": ["Validate in the specified order."]}"#.into(),
        });
        let distilled = distill(recorder.clone(), &dossiers, &[], 8, 2);
        let seen = recorder.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(distilled.calls, 2);
        for prompt in seen.iter() {
            assert!(!prompt.contains(SENTINEL), "hidden test content reached the architect");
            assert!(!prompt.contains("\"failed\""), "hidden counts reached the architect");
        }
        assert_eq!(distilled.lessons, vec!["Validate in the specified order.".to_string()]);
    }

    #[test]
    fn uninformed_dossiers_carry_requirements_only_and_match_the_failure_count() {
        let (_root, run, suite) = fixture();
        let informed = failure_dossiers(&run, &suite, "C").unwrap();
        let uninformed = uninformed_dossiers(&run, &suite, "C", informed.len(), 7).unwrap();
        assert_eq!(uninformed.len(), informed.len());
        assert!(uninformed.iter().all(|d| d.code.is_empty()));
        let prompt = uninformed_prompt(&uninformed[0]);
        assert!(prompt.contains("Errors first."));
        assert!(!prompt.contains("CODE_C_") && !prompt.contains("FAILS"));
        assert_eq!(
            uninformed_dossiers(&run, &suite, "C", 2, 7).unwrap().len(),
            2,
            "samples from every submission of the arm, pass or fail"
        );
    }

    #[test]
    fn merged_lessons_are_filtered_capped_and_kept_on_failure() {
        let (_root, run, suite) = fixture();
        let dossiers = failure_dossiers(&run, &suite, "C").unwrap();
        let ids = train_identifiers(&dossiers);
        assert!(ids.contains("parse_thing") && ids.contains("fam-x"));
        let reply = json!({"lessons": [
            "Call parse_thing last.", "Lesson one.", "Lesson two.", "Lesson three.",
            "Avoid fam-x mistakes."
        ]})
        .to_string();
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
            merge_reply: reply,
        });
        let distilled = distill(recorder, &dossiers, &[], 2, 1);
        assert_eq!(distilled.lessons, vec!["Lesson one.", "Lesson two."]);
        assert_eq!(distilled.dropped.len(), 2);
        let failing = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
            merge_reply: "not json".into(),
        });
        let kept = distill(failing, &dossiers, &["Incumbent.".to_string()], 8, 1);
        assert_eq!(kept.lessons, vec!["Incumbent.".to_string()]);
    }
}
