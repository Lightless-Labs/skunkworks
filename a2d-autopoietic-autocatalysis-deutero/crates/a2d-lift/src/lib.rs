//! `a2d lift`: the experiment controller for
//! `docs/plans/2026-10-14-lift-first-roadmap.md`.
//!
//! This is the only path that runs arms and scores them. Arms see a
//! [`task::PublicTask`] and a budgeted provider; only the controller holds
//! hidden tests. A run has two phases: every arm generates and submits first
//! (nothing hidden-derived is computed or written), then each submission is
//! scored once by the hidden oracle. Everything is recorded in a
//! hash-chained, append-only ledger.

pub mod arms;
mod cli;
pub mod ledger;
pub mod meter;
pub mod oracle;
pub mod stats;
pub mod task;
pub mod validate;

pub use cli::cli_main;

use a2d_core::provider::Provider;
use meter::{MeteredProvider, Scope, SharedMeter, lock};
use oracle::{TestScore, score_hidden, score_public};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use task::{PublicTask, Suite};

/// Paths frozen at registration: the controller, the oracle and the
/// treatment (metabolism, seed germline, providers, extractor). A
/// registration is void if anything here changed between the registered
/// commit and the run.
pub const CONTROLLER_PATHS: &[&str] = &["crates", "Cargo.toml", "Cargo.lock"];

pub const MIN_EFFECT: f64 = 0.10;
pub const DEFAULT_MIN_FAMILIES: usize = 10;
const BOOTSTRAP_ITERATIONS: usize = 10_000;
const BOOTSTRAP_SEED: u64 = 0x5eed_a2d0;

/// Source constructs a solution has no reason to use while being tested;
/// flagged on every scored submission for review, not penalised.
const SUSPICIOUS_CONSTRUCTS: &[&str] = &[
    "include_str!",
    "include!",
    "include_bytes!",
    "file!()",
    "current_exe",
    "std::fs",
    "fs::read",
    "process::exit",
    "test result:",
    "env::var",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arm {
    A,
    B,
    D,
    C,
}

impl Arm {
    pub fn label(self) -> &'static str {
        match self {
            Arm::A => "A",
            Arm::B => "B",
            Arm::D => "D",
            Arm::C => "C",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "A" | "a" => Ok(Arm::A),
            "B" | "b" => Ok(Arm::B),
            "D" | "d" => Ok(Arm::D),
            "C" | "c" => Ok(Arm::C),
            other => Err(format!("unknown arm {other:?} (expected A, B, D or C)")),
        }
    }

    /// Provider invocations this arm may spend on one task.
    pub fn budget(self, k: usize) -> usize {
        match self {
            Arm::A => 1,
            _ => k,
        }
    }
}

/// Arm C (A²D) is supplied by the CLI, which owns the metabolism wiring.
/// It must route every model call through `provider` and return each code
/// artifact it produced, in order.
pub trait ArmC: Sync {
    fn run(&self, provider: Box<dyn Provider>, task: &PublicTask, budget: usize) -> Vec<String>;
}

#[derive(Debug, Clone)]
pub struct RunPlan {
    pub arms: Vec<Arm>,
    pub k: usize,
    pub replicates: u32,
    /// Concurrent (task, replicate) tuples. Arms of one tuple run in sequence.
    pub jobs: usize,
    /// Seeds the per-tuple arm order.
    pub seed: u64,
    /// The comparator for C fixed before the run (B or D), if C runs.
    pub primary: Option<Arm>,
    pub min_families: usize,
    pub record_hidden_output: bool,
}

/// A submission from the generation phase, before hidden scoring.
#[derive(Debug, Clone)]
struct Submission {
    arm: Arm,
    task: usize,
    replicate: u32,
    code: Option<String>,
    public_green: bool,
    invocations: usize,
    tokens: usize,
    provider_ms: u64,
    coder_calls: usize,
    void_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub arm: Arm,
    pub task: String,
    pub family: String,
    pub replicate: u32,
    pub invocations: usize,
    pub tokens: usize,
    pub provider_ms: u64,
    pub coder_calls: usize,
    pub public_green: bool,
    pub void_reason: Option<String>,
    pub suspicious: Vec<String>,
    pub hidden: TestScore,
}

/// Results of a run, with what is needed to judge its completeness.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub outcomes: Vec<Outcome>,
    pub tuples_planned: usize,
    pub aborted: bool,
    /// Saved invocation files that contain a hidden test function name.
    pub hidden_name_hits: Vec<String>,
}

/// Seeded permutation of the planned arms for one tuple.
fn arm_order(plan: &RunPlan, task: &str, replicate: u32) -> Vec<Arm> {
    let mut arms = plan.arms.clone();
    let digest = ledger::sha256_hex(format!("{}:{task}:{replicate}", plan.seed).as_bytes());
    let bytes: Vec<u8> = (0..digest.len() / 2)
        .filter_map(|i| u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).ok())
        .collect();
    for i in (1..arms.len()).rev() {
        let j = bytes[i % bytes.len()] as usize % (i + 1);
        arms.swap(i, j);
    }
    arms
}

/// Phase 1 runs every arm on every tuple; phase 2 scores every submission
/// once with the hidden oracle. Tuples run on `plan.jobs` workers. When the
/// global cap or wall clock would be exceeded, no new arm run starts and the
/// run is marked aborted.
pub fn run_arms(
    plan: &RunPlan,
    suite: &Suite,
    provider: Arc<dyn Provider>,
    meter: &SharedMeter,
    out_dir: &Path,
    arm_c: Option<&dyn ArmC>,
) -> Result<RunResult, String> {
    if plan.arms.contains(&Arm::C) && arm_c.is_none() {
        return Err("arm C requested but no A²D runner is wired".to_string());
    }
    let tuples: Vec<(usize, u32)> = (0..suite.tasks.len())
        .flat_map(|task| (0..plan.replicates).map(move |replicate| (task, replicate)))
        .collect();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let submissions = Mutex::new(Vec::new());
    let error = Mutex::new(None::<String>);

    std::thread::scope(|workers| {
        for _ in 0..plan.jobs.max(1) {
            workers.spawn(|| {
                while !stop.load(Ordering::SeqCst) {
                    let Some(&(task, replicate)) = tuples.get(next.fetch_add(1, Ordering::SeqCst))
                    else {
                        break;
                    };
                    let order = arm_order(plan, &suite.tasks[task].id, replicate);
                    for (position, &arm) in order.iter().enumerate() {
                        let result = generate(
                            plan, suite, task, replicate, arm, position, &provider, meter, out_dir,
                            arm_c,
                        );
                        match result {
                            Ok(Some(submission)) => submissions.lock().unwrap().push(submission),
                            Ok(None) => {
                                stop.store(true, Ordering::SeqCst);
                                break;
                            }
                            Err(e) => {
                                error.lock().unwrap().get_or_insert(e);
                                stop.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = error.into_inner().unwrap() {
        return Err(e);
    }
    let aborted = stop.load(Ordering::SeqCst);
    let submissions = submissions.into_inner().unwrap();
    lock(meter)
        .ledger
        .append(
            "generation_complete",
            json!({"submissions": submissions.len(), "aborted": aborted}),
        )
        .map_err(|e| e.to_string())?;

    let outcomes = score_all(plan, suite, submissions, meter, out_dir)?;
    let hidden_name_hits = scan_for_hidden_names(suite, &out_dir.join("invocations"));
    Ok(RunResult {
        outcomes,
        tuples_planned: tuples.len(),
        aborted,
        hidden_name_hits,
    })
}

/// One arm on one tuple, generation only. `Ok(None)` means the run must
/// stop (cap, deadline or audit failure); the reason is in the ledger.
#[allow(clippy::too_many_arguments)]
fn generate(
    plan: &RunPlan,
    suite: &Suite,
    task_index: usize,
    replicate: u32,
    arm: Arm,
    order: usize,
    provider: &Arc<dyn Provider>,
    meter: &SharedMeter,
    out_dir: &Path,
    arm_c: Option<&dyn ArmC>,
) -> Result<Option<Submission>, String> {
    let task = &suite.tasks[task_index];
    let budget = arm.budget(plan.k);
    {
        let mut m = lock(meter);
        if let Err(reason) = m.commit(budget) {
            let _ = m.ledger.append(
                "aborted",
                json!({"reason": reason, "next": {"arm": arm.label(), "task": task.id, "replicate": replicate}}),
            );
            return Ok(None);
        }
    }
    let public = task.public();
    let scope = Scope::new(arm.label(), &task.id, replicate, budget);
    let metered = MeteredProvider::new(provider.clone(), meter.clone(), scope.clone());
    let attempts = match arm {
        Arm::A => arms::one_shot(&metered, &public),
        Arm::B => arms::best_of_k(&metered, &public, plan.k),
        Arm::D => arms::repair_loop(&metered, &public, plan.k),
        Arm::C => {
            let runner = arm_c.expect("checked by run_arms");
            let codes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runner.run(Box::new(metered), &public, plan.k)
            }))
            .unwrap_or_else(|_| {
                let _ = lock(meter).ledger.append(
                    "arm_panic",
                    json!({"arm": "C", "task": task.id, "replicate": replicate}),
                );
                Vec::new()
            });
            codes
                .into_iter()
                .enumerate()
                .map(|(index, code)| arms::Attempt {
                    index,
                    public: score_public(&public, Some(&code)),
                    code: Some(code),
                })
                .collect()
        }
    };
    lock(meter).settle(&scope);
    let submission = arms::select_submission(&attempts);
    let code = submission.and_then(|a| a.code.clone());
    let public_green = submission
        .and_then(|a| a.public.as_ref())
        .is_some_and(|s| s.all_passed);

    let dir = out_dir.join("submissions").join(arm.label()).join(&task.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("r{replicate}.rs"));
    if let Some(code) = &code {
        fs::write(&path, code).map_err(|e| e.to_string())?;
    }
    lock(meter)
        .ledger
        .append(
            "submission",
            json!({
                "arm": arm.label(),
                "task": task.id,
                "family": task.family,
                "replicate": replicate,
                "order": order,
                "attempt_index": submission.map(|a| a.index),
                "attempts": attempts.len(),
                "invocations": scope.used(),
                "coder_calls": scope.coder_calls(),
                "tokens": scope.tokens(),
                "provider_ms": scope.provider_ms(),
                "void_reason": scope.void_reason(),
                "artifact_sha256": code.as_ref().map(|c| ledger::sha256_hex(c.as_bytes())),
                "path": code.as_ref().map(|_| path.display().to_string()),
                "public": submission.and_then(|a| a.public.clone()),
            }),
        )
        .map_err(|e| e.to_string())?;
    Ok(Some(Submission {
        arm,
        task: task_index,
        replicate,
        code,
        public_green,
        invocations: scope.used(),
        tokens: scope.tokens(),
        provider_ms: scope.provider_ms(),
        coder_calls: scope.coder_calls(),
        void_reason: scope.void_reason(),
    }))
}

/// Phase 2: score every submission once with the hidden oracle.
fn score_all(
    plan: &RunPlan,
    suite: &Suite,
    submissions: Vec<Submission>,
    meter: &SharedMeter,
    out_dir: &Path,
) -> Result<Vec<Outcome>, String> {
    let next = AtomicUsize::new(0);
    let outcomes = Mutex::new(Vec::new());
    let error = Mutex::new(None::<String>);
    std::thread::scope(|workers| {
        for _ in 0..plan.jobs.max(1) {
            workers.spawn(|| {
                while let Some(sub) = submissions.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let task = &suite.tasks[sub.task];
                    let hidden = score_hidden(task, sub.code.as_deref());
                    let suspicious: Vec<String> = sub
                        .code
                        .as_deref()
                        .map(|code| {
                            SUSPICIOUS_CONSTRUCTS
                                .iter()
                                .filter(|c| code.contains(*c))
                                .map(|c| c.to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    if plan.record_hidden_output {
                        let path = out_dir
                            .join("submissions")
                            .join(sub.arm.label())
                            .join(&task.id)
                            .join(format!("r{}.hidden.txt", sub.replicate));
                        let _ = fs::write(path, &hidden.output);
                    }
                    let appended = lock(meter).ledger.append(
                        "score",
                        json!({
                            "arm": sub.arm.label(),
                            "task": task.id,
                            "family": task.family,
                            "replicate": sub.replicate,
                            "oracle": "hidden",
                            "result": hidden,
                            "suspicious": suspicious,
                        }),
                    );
                    if let Err(e) = appended {
                        error.lock().unwrap().get_or_insert(e.to_string());
                    }
                    outcomes.lock().unwrap().push(Outcome {
                        arm: sub.arm,
                        task: task.id.clone(),
                        family: task.family.clone(),
                        replicate: sub.replicate,
                        invocations: sub.invocations,
                        tokens: sub.tokens,
                        provider_ms: sub.provider_ms,
                        coder_calls: sub.coder_calls,
                        public_green: sub.public_green,
                        void_reason: sub.void_reason.clone(),
                        suspicious,
                        hidden,
                    });
                }
            });
        }
    });
    if let Some(e) = error.into_inner().unwrap() {
        return Err(e);
    }
    let mut outcomes = outcomes.into_inner().unwrap();
    outcomes.sort_by(|a, b| (&a.task, a.replicate, a.arm).cmp(&(&b.task, b.replicate, b.arm)));
    Ok(outcomes)
}

/// Saved invocation files (prompts, responses, transcripts) that mention a
/// hidden test function name of their own task. Any hit is investigated
/// before the run's results are used.
fn scan_for_hidden_names(suite: &Suite, invocations_dir: &Path) -> Vec<String> {
    let mut hits = Vec::new();
    for task in &suite.tasks {
        let names = validate::test_names(task.hidden_tests());
        let public_names = validate::test_names(&task.public_tests);
        let names: BTreeSet<_> = names.difference(&public_names).cloned().collect();
        if names.is_empty() {
            continue;
        }
        for arm_dir in fs::read_dir(invocations_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            for path in files_under(&arm_dir.path().join(&task.id)) {
                let text = fs::read_to_string(&path).unwrap_or_default();
                if names.iter().any(|name| text.contains(name.as_str())) {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    hits.sort();
    hits
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Per-arm results and paired comparisons over complete tuples only: a
/// tuple counts when every planned arm produced a non-void outcome.
pub fn summarize(result: &RunResult, plan: &RunPlan) -> Value {
    let mut complete_tuples: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut void_tuples: BTreeSet<(String, u32)> = BTreeSet::new();
    for o in &result.outcomes {
        if o.void_reason.is_some() {
            void_tuples.insert((o.task.clone(), o.replicate));
        }
    }
    for o in &result.outcomes {
        let key = (o.task.clone(), o.replicate);
        let all_arms = plan.arms.iter().all(|&arm| {
            result
                .outcomes
                .iter()
                .any(|x| x.arm == arm && x.task == o.task && x.replicate == o.replicate)
        });
        if all_arms && !void_tuples.contains(&key) {
            complete_tuples.insert(key);
        }
    }
    let rows: Vec<&Outcome> = result
        .outcomes
        .iter()
        .filter(|o| complete_tuples.contains(&(o.task.clone(), o.replicate)))
        .collect();
    let complete =
        !result.aborted && void_tuples.is_empty() && complete_tuples.len() == result.tuples_planned;

    let per_arm: Vec<Value> = plan
        .arms
        .iter()
        .map(|&arm| {
            let arm_rows: Vec<&&Outcome> = rows.iter().filter(|o| o.arm == arm).collect();
            let n = arm_rows.len();
            let passed = arm_rows.iter().filter(|o| o.hidden.all_passed).count();
            json!({
                "arm": arm.label(),
                "n": n,
                "passed": passed,
                "rate": if n == 0 { 0.0 } else { passed as f64 / n as f64 },
                "invocations": arm_rows.iter().map(|o| o.invocations).sum::<usize>(),
                "coder_calls": arm_rows.iter().map(|o| o.coder_calls).sum::<usize>(),
                "tokens": arm_rows.iter().map(|o| o.tokens).sum::<usize>(),
                "provider_ms": arm_rows.iter().map(|o| o.provider_ms).sum::<u64>(),
                "public_green_hidden_fail": arm_rows.iter().filter(|o| o.public_green && !o.hidden.all_passed).count(),
                "suspicious_submissions": arm_rows.iter().filter(|o| !o.suspicious.is_empty()).count(),
            })
        })
        .collect();

    let pairs = [
        (Arm::C, Arm::B),
        (Arm::C, Arm::D),
        (Arm::C, Arm::A),
        (Arm::B, Arm::A),
        (Arm::D, Arm::A),
        (Arm::D, Arm::B),
    ];
    let comparisons: Vec<Value> = pairs
        .iter()
        .filter(|(x, y)| plan.arms.contains(x) && plan.arms.contains(y))
        .map(|&(x, y)| {
            let paired: Vec<stats::PairedOutcome> = rows
                .iter()
                .filter(|o| o.arm == x)
                .filter_map(|ox| {
                    rows.iter()
                        .find(|oy| oy.arm == y && oy.task == ox.task && oy.replicate == ox.replicate)
                        .map(|oy| stats::PairedOutcome {
                            family: ox.family.clone(),
                            task: ox.task.clone(),
                            replicate: ox.replicate,
                            x_pass: ox.hidden.all_passed,
                            y_pass: oy.hidden.all_passed,
                        })
                })
                .collect();
            let s = stats::paired_summary(&paired, BOOTSTRAP_ITERATIONS, BOOTSTRAP_SEED);
            let class = if complete {
                format!(
                    "{:?}",
                    stats::classify_guarded(&s, MIN_EFFECT, plan.min_families)
                )
            } else {
                "Incomplete".to_string()
            };
            let role = if x == Arm::C && plan.primary == Some(y) {
                "primary"
            } else {
                "exploratory"
            };
            json!({
                "treatment": x.label(),
                "comparator": y.label(),
                "role": role,
                "n": s.n,
                "families": s.families,
                "treatment_rate": s.x_rate,
                "comparator_rate": s.y_rate,
                "lift": s.lift,
                "ci95": [s.ci_low, s.ci_high],
                "family_sign_flip_p": stats::family_sign_flip_p(&paired, BOOTSTRAP_ITERATIONS, BOOTSTRAP_SEED),
                "discordant_treatment_only": s.discordant_x_only,
                "discordant_comparator_only": s.discordant_y_only,
                "mcnemar_exact_p_descriptive": s.mcnemar_exact_p,
                "class": class,
            })
        })
        .collect();
    json!({
        "complete": complete,
        "aborted": result.aborted,
        "tuples_planned": result.tuples_planned,
        "tuples_complete": complete_tuples.len(),
        "tuples_void": void_tuples.len(),
        "hidden_name_hits": result.hidden_name_hits,
        "min_effect": MIN_EFFECT,
        "min_families": plan.min_families,
        "primary_comparator": plan.primary.map(|a| a.label()),
        "arms": per_arm,
        "comparisons": comparisons,
    })
}

#[cfg(test)]
mod tests;
