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
pub const DEFAULT_MAX_EXCLUDED_SHARE: f64 = 0.05;
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
    /// Classify only if at most this share of planned tuples is excluded
    /// (voided, aborted or missing an arm); otherwise the run is Invalid.
    pub max_excluded_share: f64,
    pub record_hidden_output: bool,
    /// Stop after generation; score later with [`score_deferred`] once the
    /// submissions are committed (confirmation protocol).
    pub defer_scoring: bool,
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
    tool_calls: usize,
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
    pub tool_calls: usize,
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
    /// False when scoring was deferred.
    pub scored: bool,
}

/// Deterministic Fisher-Yates shuffle keyed by `key` (SHA-256 counter stream).
fn seeded_shuffle<T>(items: &mut [T], key: &str) {
    for i in (1..items.len()).rev() {
        let digest = ledger::sha256_hex(format!("{key}:{i}").as_bytes());
        let draw = u64::from_str_radix(&digest[..16], 16).unwrap_or(0);
        items.swap(i, (draw % (i as u64 + 1)) as usize);
    }
}

/// Seeded permutation of the planned arms for one tuple.
fn arm_order(plan: &RunPlan, task: &str, replicate: u32) -> Vec<Arm> {
    let mut arms = plan.arms.clone();
    seeded_shuffle(&mut arms, &format!("arms:{}:{task}:{replicate}", plan.seed));
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
    // Tuples run in a seeded random order so families and levels are not
    // confounded with time (provider drift, contention).
    let mut tuples: Vec<(usize, u32)> = (0..suite.tasks.len())
        .flat_map(|task| (0..plan.replicates).map(move |replicate| (task, replicate)))
        .collect();
    seeded_shuffle(&mut tuples, &format!("tuples:{}", plan.seed));
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
    if plan.defer_scoring {
        return Ok(RunResult {
            outcomes: Vec::new(),
            tuples_planned: tuples.len(),
            aborted,
            hidden_name_hits: Vec::new(),
            scored: false,
        });
    }

    let outcomes = score_all(plan, suite, submissions, meter, out_dir)?;
    let hidden_name_hits = scan_for_hidden_names(suite, &out_dir.join("invocations"));
    Ok(RunResult {
        outcomes,
        tuples_planned: tuples.len(),
        aborted,
        hidden_name_hits,
        scored: true,
    })
}

/// Score a run whose generation phase finished with `defer_scoring`. The
/// ledger chain is verified and continued; the suite must match the
/// registered hash; every submission file must match its ledgered SHA-256;
/// a run is scored at most once.
pub fn score_deferred(
    out_dir: &Path,
    suite_dir: &Path,
    jobs: usize,
    record_hidden_output: bool,
    expected_head: Option<&str>,
) -> Result<(RunResult, RunPlan, SharedMeter), String> {
    let ledger_path = out_dir.join("ledger.jsonl");
    let ledger = ledger::Ledger::resume(&ledger_path)?;
    if let Some(expected) = expected_head
        && ledger.head() != Some(expected)
    {
        return Err(format!(
            "ledger head {} does not match the committed anchor {expected}",
            ledger.head().unwrap_or("<empty>")
        ));
    }
    let records: Vec<Value> = fs::read_to_string(&ledger_path)
        .map_err(|e| e.to_string())?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let of_kind = |kind: &'static str| records.iter().filter(move |r| r["kind"] == kind);
    if of_kind("score").next().is_some() {
        return Err("run already scored; a run is scored once".to_string());
    }
    let registration = of_kind("registration")
        .next()
        .ok_or("ledger has no registration")?;
    let generation = of_kind("generation_complete")
        .next()
        .ok_or("generation phase did not complete")?;

    let arms: Vec<Arm> = registration["arms"]
        .as_array()
        .ok_or("registration lacks arms")?
        .iter()
        .map(|a| Arm::parse(a.as_str().unwrap_or("")))
        .collect::<Result<_, _>>()?;
    let number = |key: &str| {
        registration[key]
            .as_u64()
            .ok_or(format!("registration lacks {key}"))
    };
    let plan = RunPlan {
        arms,
        k: number("k")? as usize,
        replicates: number("replicates")? as u32,
        jobs,
        seed: number("seed")?,
        primary: registration["primary_comparator"]
            .as_str()
            .map(Arm::parse)
            .transpose()?,
        min_families: number("min_families")? as usize,
        max_excluded_share: registration["max_excluded_share"]
            .as_f64()
            .unwrap_or(DEFAULT_MAX_EXCLUDED_SHARE),
        record_hidden_output,
        defer_scoring: false,
    };
    let suite = task::load_suite(suite_dir, registration["suite_hash"].as_str())?;

    let mut submissions = Vec::new();
    for r in of_kind("submission") {
        let arm = Arm::parse(r["arm"].as_str().unwrap_or(""))?;
        let task_id = r["task"].as_str().unwrap_or("");
        let task = suite
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(format!("submission for unknown task {task_id}"))?;
        let replicate = r["replicate"].as_u64().unwrap_or(0) as u32;
        let code = match r["artifact_sha256"].as_str() {
            None => None,
            Some(expected) => {
                let path = out_dir
                    .join("submissions")
                    .join(arm.label())
                    .join(task_id)
                    .join(format!("r{replicate}.rs"));
                let code = fs::read_to_string(&path)
                    .map_err(|e| format!("read {}: {e}", path.display()))?;
                if ledger::sha256_hex(code.as_bytes()) != expected {
                    return Err(format!(
                        "submission {} does not match its ledgered hash",
                        path.display()
                    ));
                }
                Some(code)
            }
        };
        let count = |key: &str| r[key].as_u64().unwrap_or(0);
        submissions.push(Submission {
            arm,
            task,
            replicate,
            code,
            public_green: r["public"]["all_passed"].as_bool().unwrap_or(false),
            invocations: count("invocations") as usize,
            tokens: count("tokens") as usize,
            provider_ms: count("provider_ms"),
            coder_calls: count("coder_calls") as usize,
            tool_calls: count("tool_calls") as usize,
            void_reason: r["void_reason"].as_str().map(str::to_string),
        });
    }

    let meter = meter::Meter::new(
        ledger,
        0,
        std::time::Duration::from_secs(365 * 24 * 3600),
        out_dir.join("invocations"),
    );
    lock(&meter)
        .ledger
        .append(
            "scoring_started",
            json!({"deferred": true, "submissions": submissions.len()}),
        )
        .map_err(|e| e.to_string())?;
    let outcomes = score_all(&plan, &suite, submissions, &meter, out_dir)?;
    let hidden_name_hits = scan_for_hidden_names(&suite, &out_dir.join("invocations"));
    let result = RunResult {
        outcomes,
        tuples_planned: suite.tasks.len() * plan.replicates as usize,
        aborted: generation["aborted"].as_bool().unwrap_or(true),
        hidden_name_hits,
        scored: true,
    };
    Ok((result, plan, meter))
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
                "tool_calls": scope.tool_calls(),
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
        tool_calls: scope.tool_calls(),
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
    // Score everything first, then append every score record: an
    // interrupted scoring phase leaves no partial scores in the ledger and
    // can simply be rerun.
    let next = AtomicUsize::new(0);
    let outcomes = Mutex::new(Vec::new());
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
                    outcomes.lock().unwrap().push(Outcome {
                        arm: sub.arm,
                        task: task.id.clone(),
                        family: task.family.clone(),
                        replicate: sub.replicate,
                        invocations: sub.invocations,
                        tokens: sub.tokens,
                        provider_ms: sub.provider_ms,
                        coder_calls: sub.coder_calls,
                        tool_calls: sub.tool_calls,
                        public_green: sub.public_green,
                        void_reason: sub.void_reason.clone(),
                        suspicious,
                        hidden,
                    });
                }
            });
        }
    });
    let mut outcomes = outcomes.into_inner().unwrap();
    outcomes.sort_by(|a, b| (&a.task, a.replicate, a.arm).cmp(&(&b.task, b.replicate, b.arm)));
    let mut m = lock(meter);
    for o in &outcomes {
        if plan.record_hidden_output {
            let path = out_dir
                .join("submissions")
                .join(o.arm.label())
                .join(&o.task)
                .join(format!("r{}.hidden.txt", o.replicate));
            let _ = fs::write(path, &o.hidden.output);
        }
        m.ledger
            .append(
                "score",
                json!({
                    "arm": o.arm.label(),
                    "task": o.task,
                    "family": o.family,
                    "replicate": o.replicate,
                    "oracle": "hidden",
                    "result": o.hidden,
                    "suspicious": o.suspicious,
                }),
            )
            .map_err(|e| e.to_string())?;
    }
    drop(m);
    Ok(outcomes)
}

/// Saved prompts that contain a hidden test function name of their own task
/// as a whole identifier. Prompts are what reaches an arm: hidden content
/// leaked through candidate output (e.g. public test output) would surface
/// in a later prompt. Responses are not scanned — a model naming its own
/// test after a specified behaviour is a false positive (E06).
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
                if !path.to_string_lossy().ends_with(".prompt.txt") {
                    continue;
                }
                let text = fs::read_to_string(&path).unwrap_or_default();
                if names
                    .iter()
                    .any(|name| contains_identifier(&text, name.as_str()))
                {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    hits.sort();
    hits
}

/// `name` occurs in `text` delimited by non-identifier characters.
fn contains_identifier(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
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
    let excluded_share = if result.tuples_planned == 0 {
        1.0
    } else {
        1.0 - complete_tuples.len() as f64 / result.tuples_planned as f64
    };
    let classifiable = excluded_share <= plan.max_excluded_share + 1e-12;
    let fraction = |o: &Outcome| {
        if o.hidden.expected == 0 {
            0.0
        } else {
            o.hidden.passed as f64 / o.hidden.expected as f64
        }
    };
    // Integrity findings block classification until reviewed.
    let tool_calls: usize = result.outcomes.iter().map(|o| o.tool_calls).sum();
    let integrity_ok = result.hidden_name_hits.is_empty() && tool_calls == 0;

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
                "tool_calls": arm_rows.iter().map(|o| o.tool_calls).sum::<usize>(),
                "tokens": arm_rows.iter().map(|o| o.tokens).sum::<usize>(),
                "provider_ms": arm_rows.iter().map(|o| o.provider_ms).sum::<u64>(),
                "public_green_hidden_fail": arm_rows.iter().filter(|o| o.public_green && !o.hidden.all_passed).count(),
                "hidden_fraction_mean": if n == 0 { 0.0 } else { arm_rows.iter().map(|o| fraction(o)).sum::<f64>() / n as f64 },
                "hidden_compile_failures": arm_rows.iter().filter(|o| !o.hidden.compiled).count(),
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
            let fraction_diffs: Vec<f64> = rows
                .iter()
                .filter(|o| o.arm == x)
                .filter_map(|ox| {
                    rows.iter()
                        .find(|oy| oy.arm == y && oy.task == ox.task && oy.replicate == ox.replicate)
                        .map(|oy| fraction(ox) - fraction(oy))
                })
                .collect();
            let fraction_lift = if fraction_diffs.is_empty() {
                0.0
            } else {
                fraction_diffs.iter().sum::<f64>() / fraction_diffs.len() as f64
            };
            let class = if !integrity_ok {
                "IntegrityReview".to_string()
            } else if !classifiable {
                "Invalid".to_string()
            } else {
                format!(
                    "{:?}",
                    stats::classify_guarded(&s, MIN_EFFECT, plan.min_families)
                )
            };
            // Primary: C vs the pre-registered comparator. Secondary: C vs
            // the other of B and D (the overall SUPPORTED claim needs both).
            let role = if x == Arm::C && plan.primary == Some(y) {
                "primary"
            } else if x == Arm::C && plan.primary.is_some() && matches!(y, Arm::B | Arm::D) {
                "secondary"
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
                "hidden_fraction_lift": fraction_lift,
                "class": class,
            })
        })
        .collect();
    // Overall claim (roadmap Addendum 2026-10-14, P1 gate): SUPPORTED needs
    // the primary AND the secondary comparison SUPPORTED; RULED OUT
    // (falsification) is decided by the primary comparison alone.
    let class_of = |role: &str| {
        comparisons
            .iter()
            .find(|c| c["role"] == role)
            .and_then(|c| c["class"].as_str())
            .map(str::to_string)
    };
    let claim = match (
        class_of("primary").as_deref(),
        class_of("secondary").as_deref(),
    ) {
        (None, _) => "NoPrimary".to_string(),
        (Some("Supported"), None | Some("Supported")) => "SUPPORTED".to_string(),
        (Some("RuledOut"), _) => "RULED OUT".to_string(),
        (Some(c @ ("IntegrityReview" | "Invalid")), _) => c.to_string(),
        _ => "INCONCLUSIVE".to_string(),
    };
    json!({
        "claim": claim,
        "excluded_share": excluded_share,
        "max_excluded_share": plan.max_excluded_share,
        "complete": complete,
        "aborted": result.aborted,
        "tuples_planned": result.tuples_planned,
        "tuples_complete": complete_tuples.len(),
        "tuples_void": void_tuples.len(),
        "hidden_name_hits": result.hidden_name_hits,
        "coder_tool_calls": tool_calls,
        "integrity_ok": integrity_ok,
        "min_effect": MIN_EFFECT,
        "min_families": plan.min_families,
        "primary_comparator": plan.primary.map(|a| a.label()),
        "arms": per_arm,
        "comparisons": comparisons,
    })
}

#[cfg(test)]
mod tests;
