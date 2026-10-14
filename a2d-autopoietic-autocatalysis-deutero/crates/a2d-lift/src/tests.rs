//! End-to-end controller tests on mock providers.

use super::*;
use crate::ledger::Ledger;
use crate::meter::Meter;
use crate::task::Task;
use a2d_core::provider::{InvocationRequest, InvocationResponse, ProviderError, TokenUsage};
use a2d_core::types::EnzymeId;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

const SENTINEL: &str = "HIDDEN_SENTINEL_e7c1";
const GOOD: &str = "```rust\nfn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n```";
const WRONG: &str = "```rust\nfn double(x: i32) -> i32 { x + 1 }\nfn main() {}\n```";
const PUBLIC: &str = "#[test]\nfn doubles_two() { assert_eq!(double(2), 4); }\n";

fn hidden() -> String {
    format!(
        "#[test]\nfn doubles_negative() {{\n    // {SENTINEL}\n    assert_eq!(double(-3), -6);\n}}\n"
    )
}

type Responder = dyn Fn(&InvocationRequest, usize) -> String + Send + Sync;

/// Records every request it sees; answers via `respond(request, call_index)`.
struct RecordingMock {
    seen: Arc<Mutex<Vec<String>>>,
    respond: Box<Responder>,
}

impl Provider for RecordingMock {
    fn name(&self) -> &str {
        "mock/recording"
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        let mut seen = self.seen.lock().unwrap();
        let index = seen.len();
        seen.push(format!("{}\n{}", request.system, request.prompt));
        drop(seen);
        Ok(InvocationResponse {
            text: (self.respond)(request, index),
            raw_output: None,
            tool_events: Vec::new(),
            transcript: None,
            thinking: None,
            usage: TokenUsage::default(),
        })
    }
}

fn mock(
    respond: impl Fn(&InvocationRequest, usize) -> String + Send + Sync + 'static,
) -> (Arc<dyn Provider>, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let provider = RecordingMock {
        seen: seen.clone(),
        respond: Box::new(respond),
    };
    (Arc::new(provider), seen)
}

fn suite() -> Suite {
    Suite {
        root: PathBuf::from("mem"),
        hash: "test".to_string(),
        tasks: vec![
            Task::for_test("t1", "fam-a", PUBLIC, &hidden()),
            Task::for_test("t2", "fam-b", PUBLIC, &hidden()),
        ],
    }
}

fn meter_in(dir: &Path, cap: usize) -> SharedMeter {
    let ledger = Ledger::create(&dir.join("ledger.jsonl")).unwrap();
    Meter::new(
        ledger,
        cap,
        Duration::from_secs(600),
        dir.join("invocations"),
    )
}

fn ledger_records(dir: &Path) -> Vec<Value> {
    fs::read_to_string(dir.join("ledger.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// A stand-in for the metabolism: several enzymes share the budget, and the
/// coder sees the public score of its previous attempt as "feedback".
struct MockArmC {
    attempts_wanted: usize,
}

impl ArmC for MockArmC {
    fn run(&self, provider: Box<dyn Provider>, task: &PublicTask, _budget: usize) -> Vec<String> {
        let mut codes = Vec::new();
        for i in 0..self.attempts_wanted {
            let enzyme = if i % 2 == 0 { "coder" } else { "evolver" };
            let request = InvocationRequest {
                enzyme_id: EnzymeId::from(enzyme),
                system: format!("enzyme {enzyme}"),
                prompt: arms::task_prompt(task),
                max_tokens: 1000,
            };
            match provider.invoke(&request) {
                Ok(response) => {
                    if let Some(code) = a2d_core::benchmark::extract_rust_code_pub(&response.text) {
                        codes.push(code);
                    }
                }
                Err(_) => break,
            }
        }
        codes
    }
}

#[test]
fn hidden_tests_never_reach_any_arm_and_are_scored_once() {
    let dir = tempfile::tempdir().unwrap();
    let meter = meter_in(dir.path(), 100);
    // Wrong first, right afterwards: B, C and D can recover, A cannot.
    let (provider, seen) = mock(|_, index| if index % 4 == 0 { WRONG } else { GOOD }.to_string());
    let plan = RunPlan {
        arms: vec![Arm::A, Arm::B, Arm::D, Arm::C],
        k: 4,
        replicates: 1,
        record_hidden_output: false,
    };
    let arm_c = MockArmC { attempts_wanted: 4 };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), Some(&arm_c)).unwrap();
    assert_eq!(outcomes.len(), 8);

    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty());
    for request in seen.iter() {
        assert!(
            !request.contains(SENTINEL),
            "hidden test reached an arm:\n{request}"
        );
        assert!(
            !request.contains("doubles_negative"),
            "hidden test name reached an arm"
        );
    }
    // Saved invocation artifacts and submissions carry no hidden content either.
    for entry in walk(dir.path()) {
        let text = fs::read_to_string(&entry).unwrap_or_default();
        if !entry.ends_with("ledger.jsonl") {
            assert!(
                !text.contains(SENTINEL),
                "hidden content in {}",
                entry.display()
            );
        }
    }

    let records = ledger_records(dir.path());
    let scores: Vec<&Value> = records.iter().filter(|r| r["kind"] == "score").collect();
    assert_eq!(scores.len(), 8, "one hidden score per arm × task");
    let submissions = records.iter().filter(|r| r["kind"] == "submission").count();
    assert_eq!(submissions, 8);

    let summary = summarize(&outcomes, &plan.arms);
    let rate = |arm: &str| {
        summary["arms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["arm"] == arm)
            .unwrap()["rate"]
            .as_f64()
            .unwrap()
    };
    assert!(rate("B") >= rate("A"));
    assert!(summary["comparisons"].as_array().unwrap().len() >= 4);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn arm_c_budget_is_enforced_across_all_enzymes() {
    let dir = tempfile::tempdir().unwrap();
    let meter = meter_in(dir.path(), 100);
    let (provider, seen) = mock(|_, _| GOOD.to_string());
    let plan = RunPlan {
        arms: vec![Arm::C],
        k: 4,
        replicates: 1,
        record_hidden_output: false,
    };
    let greedy = MockArmC {
        attempts_wanted: 10,
    };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), Some(&greedy)).unwrap();
    assert!(outcomes.iter().all(|o| o.invocations == 4));
    assert_eq!(seen.lock().unwrap().len(), 8, "4 per task, never more");
    let records = ledger_records(dir.path());
    assert_eq!(
        records.iter().filter(|r| r["kind"] == "invocation").count(),
        8
    );
    assert!(
        records
            .iter()
            .any(|r| r["kind"] == "invocation_refused" && r["reason"] == "arm budget exhausted")
    );
}

#[test]
fn repair_loop_feeds_back_public_failures_and_stops_when_green() {
    let dir = tempfile::tempdir().unwrap();
    let meter = meter_in(dir.path(), 100);
    let (provider, seen) = mock(|request, _| {
        if request.prompt.contains("Previous attempt") {
            GOOD
        } else {
            WRONG
        }
        .to_string()
    });
    let plan = RunPlan {
        arms: vec![Arm::D],
        k: 4,
        replicates: 1,
        record_hidden_output: false,
    };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), None).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|o| o.invocations == 2 && o.hidden.all_passed)
    );
    let seen = seen.lock().unwrap();
    let repair = seen
        .iter()
        .find(|r| r.contains("Previous attempt"))
        .unwrap();
    assert!(
        repair.contains("doubles_two"),
        "public failure fed back:\n{repair}"
    );
    assert!(!repair.contains(SENTINEL));
}

#[test]
fn run_aborts_before_exceeding_global_cap() {
    let dir = tempfile::tempdir().unwrap();
    let meter = meter_in(dir.path(), 5);
    let (provider, seen) = mock(|_, _| GOOD.to_string());
    let plan = RunPlan {
        arms: vec![Arm::A, Arm::B],
        k: 4,
        replicates: 1,
        record_hidden_output: false,
    };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), None).unwrap();
    // t1: A (1) + B (4) = 5 = cap; t2's A cannot fit, so the run aborts there.
    assert_eq!(seen.lock().unwrap().len(), 5);
    assert_eq!(outcomes.len(), 2);
    let records = ledger_records(dir.path());
    assert!(records.iter().any(|r| r["kind"] == "aborted"));
}

#[test]
fn arm_c_without_runner_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let meter = meter_in(dir.path(), 5);
    let (provider, _) = mock(|_, _| GOOD.to_string());
    let plan = RunPlan {
        arms: vec![Arm::C],
        k: 1,
        replicates: 1,
        record_hidden_output: false,
    };
    assert!(run_arms(&plan, &suite(), provider, &meter, dir.path(), None).is_err());
}
