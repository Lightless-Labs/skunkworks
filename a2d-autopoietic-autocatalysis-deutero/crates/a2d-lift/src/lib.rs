//! `a2d lift`: the experiment controller for
//! `docs/plans/2026-10-14-lift-first-roadmap.md`.
//!
//! This is the only path that runs arms and scores them. Arms see a
//! [`task::PublicTask`] and a budgeted provider; only the controller holds
//! hidden tests, and it scores each final submission once. Everything is
//! recorded in an append-only ledger.

pub mod arms;
mod cli;
pub mod ledger;
pub mod meter;
pub mod oracle;
pub mod stats;
pub mod task;

pub use cli::cli_main;

use a2d_core::provider::Provider;
use meter::{MeteredProvider, SharedMeter, lock};
use oracle::{TestScore, score_hidden, score_public};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use task::{PublicTask, Suite};

/// Controller and oracle files. A registration is void if any of these
/// changed between the registered commit and the run.
pub const CONTROLLER_PATHS: &[&str] = &[
    "crates/a2d-lift",
    "crates/a2d-core/src/sandbox.rs",
    "crates/a2d-cli/src/lift_arm_c.rs",
];

pub const MIN_EFFECT: f64 = 0.10;
const BOOTSTRAP_ITERATIONS: usize = 10_000;
const BOOTSTRAP_SEED: u64 = 0x5eed_a2d0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
pub trait ArmC {
    fn run(&self, provider: Box<dyn Provider>, task: &PublicTask, budget: usize) -> Vec<String>;
}

#[derive(Debug, Clone)]
pub struct RunPlan {
    pub arms: Vec<Arm>,
    pub k: usize,
    pub replicates: u32,
    pub record_hidden_output: bool,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub arm: Arm,
    pub task: String,
    pub family: String,
    pub replicate: u32,
    pub invocations: usize,
    pub hidden: TestScore,
}

/// Run every planned arm on every task and replicate, scoring each final
/// submission once with the hidden oracle.
pub fn run_arms(
    plan: &RunPlan,
    suite: &Suite,
    provider: Arc<dyn Provider>,
    meter: &SharedMeter,
    out_dir: &Path,
    arm_c: Option<&dyn ArmC>,
) -> Result<Vec<Outcome>, String> {
    if plan.arms.contains(&Arm::C) && arm_c.is_none() {
        return Err("arm C requested but no A²D runner is wired".to_string());
    }
    let mut outcomes = Vec::new();
    'run: for task in &suite.tasks {
        let public = task.public();
        for replicate in 0..plan.replicates {
            for &arm in &plan.arms {
                {
                    let mut m = lock(meter);
                    let reason = if m.past_deadline() {
                        Some("wall-clock cap reached")
                    } else if m.global_used() + arm.budget(plan.k) > m.global_cap() {
                        Some("global invocation cap would be exceeded")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        let _ = m.ledger.append(
                            "aborted",
                            json!({"reason": reason, "next": {"arm": arm.label(), "task": task.id, "replicate": replicate}}),
                        );
                        break 'run;
                    }
                    m.begin(arm.label(), &task.id, replicate, arm.budget(plan.k));
                }
                let metered = MeteredProvider::new(provider.clone(), meter.clone());
                let attempts = match arm {
                    Arm::A => arms::one_shot(&metered, &public),
                    Arm::B => arms::best_of_k(&metered, &public, plan.k),
                    Arm::D => arms::repair_loop(&metered, &public, plan.k),
                    Arm::C => arm_c
                        .expect("checked above")
                        .run(Box::new(metered), &public, plan.k)
                        .into_iter()
                        .enumerate()
                        .map(|(index, code)| arms::Attempt {
                            index,
                            public: score_public(&public, Some(&code)),
                            code: Some(code),
                        })
                        .collect(),
                };
                let invocations = lock(meter).end();
                let submission = arms::select_submission(&attempts);
                let code = submission.and_then(|a| a.code.as_deref());

                let dir = out_dir.join("submissions").join(arm.label()).join(&task.id);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("r{replicate}.rs"));
                if let Some(code) = code {
                    fs::write(&path, code).map_err(|e| e.to_string())?;
                }
                let base = json!({
                    "arm": arm.label(),
                    "task": task.id,
                    "family": task.family,
                    "replicate": replicate,
                });
                let mut m = lock(meter);
                m.ledger
                    .append(
                        "submission",
                        merge(
                            &base,
                            json!({
                                "attempt_index": submission.map(|a| a.index),
                                "attempts": attempts.len(),
                                "invocations": invocations,
                                "artifact_sha256": code.map(|c| ledger::sha256_hex(c.as_bytes())),
                                "path": code.map(|_| path.display().to_string()),
                                "public": submission.and_then(|a| a.public.clone()),
                            }),
                        ),
                    )
                    .map_err(|e| e.to_string())?;
                drop(m);

                let hidden = score_hidden(task, code);
                if plan.record_hidden_output {
                    let _ = fs::write(dir.join(format!("r{replicate}.hidden.txt")), &hidden.output);
                }
                lock(meter)
                    .ledger
                    .append(
                        "score",
                        merge(&base, json!({"oracle": "hidden", "result": hidden})),
                    )
                    .map_err(|e| e.to_string())?;
                outcomes.push(Outcome {
                    arm,
                    task: task.id.clone(),
                    family: task.family.clone(),
                    replicate,
                    invocations,
                    hidden,
                });
            }
        }
    }
    Ok(outcomes)
}

fn merge(base: &Value, extra: Value) -> Value {
    let mut merged = base.clone();
    if let (Some(target), Value::Object(extra)) = (merged.as_object_mut(), extra) {
        target.extend(extra);
    }
    merged
}

/// Per-arm hidden pass rates and paired comparisons for every arm pair
/// present, with the treatment arm first.
pub fn summarize(outcomes: &[Outcome], arms_run: &[Arm]) -> Value {
    let per_arm: Vec<Value> = arms_run
        .iter()
        .map(|&arm| {
            let rows: Vec<&Outcome> = outcomes.iter().filter(|o| o.arm == arm).collect();
            let passed = rows.iter().filter(|o| o.hidden.all_passed).count();
            json!({
                "arm": arm.label(),
                "n": rows.len(),
                "passed": passed,
                "rate": if rows.is_empty() { 0.0 } else { passed as f64 / rows.len() as f64 },
                "invocations": rows.iter().map(|o| o.invocations).sum::<usize>(),
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
        .filter(|(x, y)| arms_run.contains(x) && arms_run.contains(y))
        .map(|&(x, y)| {
            let paired: Vec<stats::PairedOutcome> = outcomes
                .iter()
                .filter(|o| o.arm == x)
                .filter_map(|ox| {
                    outcomes
                        .iter()
                        .find(|oy| {
                            oy.arm == y && oy.task == ox.task && oy.replicate == ox.replicate
                        })
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
            json!({
                "treatment": x.label(),
                "comparator": y.label(),
                "n": s.n,
                "families": s.families,
                "treatment_rate": s.x_rate,
                "comparator_rate": s.y_rate,
                "lift": s.lift,
                "ci95": [s.ci_low, s.ci_high],
                "discordant_treatment_only": s.discordant_x_only,
                "discordant_comparator_only": s.discordant_y_only,
                "mcnemar_exact_p": s.mcnemar_exact_p,
                "class": format!("{:?}", stats::classify(&s, MIN_EFFECT)),
            })
        })
        .collect();
    json!({"arms": per_arm, "comparisons": comparisons, "min_effect": MIN_EFFECT})
}

#[cfg(test)]
mod tests;
