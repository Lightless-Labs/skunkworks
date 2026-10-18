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

#[test]
fn variant_labels_accept_safe_names_and_preserve_identity() {
    for label in ["C:a", "C:h0-v5", "D:lessons", "D:0", "X:nano", "X:0"] {
        assert_eq!(Arm::parse(label).unwrap().label(), label);
    }
    for label in [
        "C:", "D:", "C:-a", "C:A", "D:a_b", "C:a/b", "A:a", "C:a:b", "X:", "X:A", "X:a/b",
    ] {
        assert!(Arm::parse(label).is_err(), "{label}");
    }
}

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
    fn run(
        &self,
        provider: Box<dyn Provider>,
        task: &PublicTask,
        _budget: usize,
        _harness: &Value,
    ) -> Vec<String> {
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
        jobs: 1,
        seed: 0,
        primary: Some(Arm::B),
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let arm_c = MockArmC { attempts_wanted: 4 };
    let result = run_arms(&plan, &suite(), provider, &meter, dir.path(), Some(&arm_c)).unwrap();
    let outcomes = &result.outcomes;
    assert_eq!(outcomes.len(), 8);
    assert!(
        result.hidden_name_hits.is_empty(),
        "{:?}",
        result.hidden_name_hits
    );

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

    let summary = summarize(&result, &plan);
    assert_eq!(summary["complete"], true);
    let primary: Vec<&Value> = summary["comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["role"] == "primary")
        .collect();
    assert_eq!(primary.len(), 1);
    assert_eq!(primary[0]["comparator"], "B");
    // Two families < any sensible floor: never a confirmatory class.
    assert_ne!(primary[0]["class"], "Supported");

    // Generation completes before any hidden score is computed or written.
    let kinds: Vec<String> = records
        .iter()
        .map(|r| r["kind"].as_str().unwrap().to_string())
        .collect();
    let generation_done = kinds
        .iter()
        .position(|k| k == "generation_complete")
        .unwrap();
    assert!(kinds[..generation_done].iter().all(|k| k != "score"));
    assert!(kinds[generation_done..].iter().all(|k| k != "submission"));
    assert!(crate::ledger::verify_chain(&dir.path().join("ledger.jsonl")).is_ok());
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
        jobs: 1,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let greedy = MockArmC {
        attempts_wanted: 10,
    };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), Some(&greedy))
        .unwrap()
        .outcomes;
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
        jobs: 1,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let outcomes = run_arms(&plan, &suite(), provider, &meter, dir.path(), None)
        .unwrap()
        .outcomes;
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
        jobs: 1,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let result = run_arms(&plan, &suite(), provider, &meter, dir.path(), None).unwrap();
    assert!(result.aborted);
    assert_eq!(summarize(&result, &plan)["complete"], false);
    let outcomes = result.outcomes;
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
        jobs: 1,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    assert!(run_arms(&plan, &suite(), provider, &meter, dir.path(), None).is_err());
}

#[test]
fn arm_order_is_a_seeded_permutation_per_tuple() {
    let plan = RunPlan {
        arms: vec![Arm::A, Arm::B, Arm::D, Arm::C],
        k: 4,
        replicates: 1,
        jobs: 1,
        seed: 7,
        primary: Some(Arm::B),
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let orders: Vec<Vec<Arm>> = (0..20)
        .map(|i| arm_order(&plan, &format!("task-{i}"), 0))
        .collect();
    for order in &orders {
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![Arm::A, Arm::B, Arm::D, Arm::C]);
    }
    assert!(orders.iter().any(|o| o != &orders[0]), "order never varies");
    assert_eq!(orders[3], arm_order(&plan, "task-3", 0));
}

#[test]
fn wall_clock_refusal_mid_arm_voids_the_tuple() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::create(&dir.path().join("ledger.jsonl")).unwrap();
    let meter = Meter::new(
        ledger,
        100,
        Duration::from_millis(300),
        dir.path().join("invocations"),
    );
    let (provider, _) = mock(|_, _| {
        std::thread::sleep(Duration::from_millis(400));
        WRONG.to_string()
    });
    let plan = RunPlan {
        arms: vec![Arm::B],
        k: 3,
        replicates: 1,
        jobs: 1,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    };
    let one_task = Suite {
        root: PathBuf::from("mem"),
        hash: "test".to_string(),
        tasks: vec![Task::for_test("t1", "fam-a", PUBLIC, &hidden())],
    };
    let result = run_arms(&plan, &one_task, provider, &meter, dir.path(), None).unwrap();
    assert_eq!(result.outcomes.len(), 1);
    assert_eq!(
        result.outcomes[0].void_reason.as_deref(),
        Some("wall-clock cap reached")
    );
    let summary = summarize(&result, &plan);
    assert_eq!(summary["tuples_void"], 1);
    assert_eq!(summary["complete"], false);
}

#[test]
fn deferred_scoring_scores_once_after_generation_and_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let suite_dir = tempfile::tempdir().unwrap();
    for id in ["t1", "t2"] {
        let task_dir = suite_dir.path().join(id);
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(
            task_dir.join("task.json"),
            format!(r#"{{"id":"{id}","family":"fam-{id}","requirements":"Implement double."}}"#),
        )
        .unwrap();
        fs::write(task_dir.join("public_tests.rs"), PUBLIC).unwrap();
        fs::write(task_dir.join("hidden_tests.rs"), hidden()).unwrap();
    }
    let suite = crate::task::load_suite(suite_dir.path(), None).unwrap();
    let mut ledger = Ledger::create(&dir.path().join("ledger.jsonl")).unwrap();
    ledger
        .append(
            "registration",
            json!({"arms": ["A", "B"], "k": 2, "replicates": 1, "seed": 0,
                   "primary_comparator": null, "min_families": 1, "suite_hash": suite.hash}),
        )
        .unwrap();
    let meter = Meter::new(
        ledger,
        100,
        Duration::from_secs(600),
        dir.path().join("invocations"),
    );
    let (provider, _) = mock(|_, _| GOOD.to_string());
    let plan = RunPlan {
        arms: vec![Arm::A, Arm::B],
        k: 2,
        replicates: 1,
        jobs: 2,
        seed: 0,
        primary: None,
        min_families: 1,
        max_excluded_share: 0.0,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: true,
    };
    let generated = run_arms(&plan, &suite, provider, &meter, dir.path(), None).unwrap();
    assert!(!generated.scored && generated.outcomes.is_empty());
    drop(meter);
    assert!(
        !ledger_records(dir.path())
            .iter()
            .any(|r| r["kind"] == "score")
    );

    // Tampering with a committed submission is refused.
    let submission = dir.path().join("submissions/A/t1/r0.rs");
    let original = fs::read_to_string(&submission).unwrap();
    fs::write(&submission, "fn double(x: i32) -> i32 { x * 2 } // edited").unwrap();
    let err = score_deferred(dir.path(), suite_dir.path(), 2, false, None)
        .err()
        .unwrap();
    assert!(err.contains("ledgered hash"), "{err}");
    fs::write(&submission, original).unwrap();

    let (result, plan, _meter) =
        score_deferred(dir.path(), suite_dir.path(), 2, false, None).unwrap();
    assert_eq!(result.outcomes.len(), 4);
    assert!(result.outcomes.iter().all(|o| o.hidden.all_passed));
    assert_eq!(summarize(&result, &plan)["complete"], true);
    assert!(crate::ledger::verify_chain(&dir.path().join("ledger.jsonl")).is_ok());
    let err = score_deferred(dir.path(), suite_dir.path(), 2, false, None)
        .err()
        .unwrap();
    assert!(err.contains("already scored"), "{err}");
}

#[test]
fn deferred_scoring_requires_the_committed_ledger_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::create(&dir.path().join("ledger.jsonl")).unwrap();
    ledger.append("registration", json!({})).unwrap();
    drop(ledger);
    let err = score_deferred(dir.path(), dir.path(), 1, false, Some("not-the-head"))
        .err()
        .unwrap();
    assert!(err.contains("committed anchor"), "{err}");
}

#[test]
fn seeded_shuffle_is_a_deterministic_permutation() {
    let mut a: Vec<usize> = (0..50).collect();
    let mut b = a.clone();
    seeded_shuffle(&mut a, "k");
    seeded_shuffle(&mut b, "k");
    assert_eq!(a, b);
    let mut sorted = a.clone();
    sorted.sort();
    assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    assert_ne!(a, (0..50).collect::<Vec<_>>());
}

fn outcome(arm: Arm, family: usize, task: usize, pass: bool) -> Outcome {
    Outcome {
        arm,
        task: format!("t{family}-{task}"),
        family: format!("f{family}"),
        replicate: 0,
        invocations: 1,
        tokens: 0,
        provider_ms: 0,
        coder_calls: 1,
        tool_calls: 0,
        public_green: pass,
        void_reason: None,
        suspicious: Vec::new(),
        hidden: crate::oracle::TestScore {
            compiled: true,
            passed: if pass { 4 } else { 2 },
            failed: if pass { 0 } else { 2 },
            expected: 4,
            timed_out: false,
            exit_success: pass,
            all_passed: pass,
            output: String::new(),
        },
    }
}

fn claim_plan() -> RunPlan {
    RunPlan {
        arms: vec![Arm::B, Arm::D, Arm::C],
        k: 5,
        replicates: 1,
        jobs: 1,
        seed: 0,
        primary: Some(Arm::D),
        min_families: 10,
        max_excluded_share: 0.05,
        record_hidden_output: false,
        harness: json!({}),
        harnesses: Default::default(),
        d_notes: Default::default(),
        externals: Default::default(),
        lanes: Default::default(),
        claim: None,
        defer_scoring: false,
    }
}

/// 12 families × 4 tasks; C passes everything, the comparators pass
/// `b_pass` / `d_pass` of every family's tasks.
fn claim_result(b_pass: usize, d_pass: usize, drop_tuples: usize) -> RunResult {
    let mut outcomes = Vec::new();
    for family in 0..12 {
        for task in 0..4 {
            if family * 4 + task < drop_tuples {
                continue;
            }
            outcomes.push(outcome(Arm::C, family, task, true));
            outcomes.push(outcome(Arm::B, family, task, task < b_pass));
            outcomes.push(outcome(Arm::D, family, task, task < d_pass));
        }
    }
    RunResult {
        outcomes,
        tuples_planned: 48,
        aborted: false,
        hidden_name_hits: Vec::new(),
        scored: true,
    }
}

#[test]
fn overall_claim_needs_primary_and_secondary_supported() {
    let plan = claim_plan();
    // C beats D and B by 50 pp in every family.
    let summary = summarize(&claim_result(2, 2, 0), &plan);
    assert_eq!(summary["claim"], "SUPPORTED", "{summary}");
    let roles: Vec<&str> = summary["comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["treatment"] == "C")
        .map(|c| c["role"].as_str().unwrap())
        .collect();
    assert!(roles.contains(&"primary") && roles.contains(&"secondary"));
    // C ties B everywhere: the secondary fails, so no overall SUPPORTED.
    let summary = summarize(&claim_result(4, 2, 0), &plan);
    assert_eq!(summary["claim"], "INCONCLUSIVE", "{summary}");
}

#[test]
fn small_exclusions_still_classify_but_large_ones_are_invalid() {
    let plan = claim_plan();
    // 2 of 48 tuples missing (4.2%): still classified.
    let summary = summarize(&claim_result(2, 2, 2), &plan);
    assert_eq!(summary["complete"], false);
    assert_eq!(summary["claim"], "SUPPORTED", "{summary}");
    // 4 of 48 missing (8.3%): Invalid.
    let summary = summarize(&claim_result(2, 2, 4), &plan);
    assert_eq!(summary["claim"], "Invalid", "{summary}");
}

#[test]
fn hidden_name_scan_matches_whole_identifiers_only() {
    assert!(contains_identifier(
        "fn doubles_negative() {}",
        "doubles_negative"
    ));
    assert!(!contains_identifier(
        "fn doubles_negative_twice() {}",
        "doubles_negative"
    ));
    assert!(!contains_identifier(
        "my_doubles_negative",
        "doubles_negative"
    ));
}

fn synthetic_ledger(path: &Path, seed: u64, c_passes: usize) {
    let mut ledger = Ledger::create(path).unwrap();
    let tasks: Vec<String> = (0..4).map(|i| format!("t{i}")).collect();
    ledger
        .append(
            "registration",
            json!({"arms": ["D", "C"], "k": 5, "model": "m", "effort": "low",
                   "primary_comparator": "D", "suite_hash": "h", "seed": seed,
                   "min_families": 1, "max_excluded_share": 0.05, "tasks": tasks}),
        )
        .unwrap();
    for (i, task) in tasks.iter().enumerate() {
        for arm in ["D", "C"] {
            ledger
                .append(
                    "submission",
                    json!({"arm": arm, "task": task, "invocations": 1, "tokens": 10,
                           "public": {"all_passed": true}}),
                )
                .unwrap();
        }
        for (arm, pass) in [("D", false), ("C", i < c_passes)] {
            ledger
                .append(
                    "score",
                    json!({"arm": arm, "task": task, "family": format!("f{}", i % 2),
                           "result": {"compiled": true, "passed": 1, "failed": 0, "expected": 1,
                                      "timed_out": false, "exit_success": pass, "all_passed": pass},
                           "suspicious": []}),
                )
                .unwrap();
        }
    }
    ledger
        .append("generation_complete", json!({"aborted": false}))
        .unwrap();
    ledger
        .append("summary", json!({"hidden_name_hits": []}))
        .unwrap();
}

#[test]
fn pooling_treats_each_ledger_as_a_replicate() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.jsonl"), dir.path().join("b.jsonl"));
    synthetic_ledger(&a, 7, 4);
    synthetic_ledger(&b, 8, 2);
    let (result, plan) = pool_ledgers(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(plan.replicates, 2);
    assert_eq!(result.tuples_planned, 8);
    assert_eq!(result.outcomes.len(), 16);
    assert!(result.outcomes.iter().any(|o| o.replicate == 1));
    let summary = summarize(&result, &plan);
    assert_eq!(summary["complete"], true);
    let primary = summary["comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["role"] == "primary")
        .unwrap()
        .clone();
    assert_eq!(primary["n"], 8);
    assert_eq!(primary["families"], 2);
    assert!((primary["lift"].as_f64().unwrap() - 0.75).abs() < 1e-9);

    // A ledger from a different design is refused.
    let c = dir.path().join("c.jsonl");
    let mut other = Ledger::create(&c).unwrap();
    other
        .append(
            "registration",
            json!({"arms": ["D", "C"], "k": 9, "model": "m", "effort": "low",
                                       "primary_comparator": "D", "suite_hash": "h", "tasks": []}),
        )
        .unwrap();
    drop(other);
    let err = pool_ledgers(&[a, c]).err().unwrap();
    assert!(err.contains("'k' differs"), "{err}");
}

#[derive(Default)]
struct HarnessMock {
    seen: Mutex<Vec<(Value, usize)>>,
}

impl ArmC for HarnessMock {
    fn validate_harness(&self, harness: &Value) -> Result<(), String> {
        if harness["invalid"] == true {
            return Err("invalid mock harness".to_string());
        }
        Ok(())
    }

    fn run(
        &self,
        provider: Box<dyn Provider>,
        task: &PublicTask,
        budget: usize,
        harness: &Value,
    ) -> Vec<String> {
        self.seen.lock().unwrap().push((harness.clone(), budget));
        MockArmC {
            attempts_wanted: 10,
        }
        .run(provider, task, budget, harness)
    }
}

fn variant_args(dir: &Path, arms: &str) -> Vec<String> {
    let task = dir.join("suite/t1");
    fs::create_dir_all(&task).unwrap();
    fs::write(
        task.join("task.json"),
        r#"{"id":"t1","family":"fam-a","requirements":"Implement double."}"#,
    )
    .unwrap();
    fs::write(task.join("public_tests.rs"), PUBLIC).unwrap();
    fs::write(task.join("hidden_tests.rs"), hidden()).unwrap();
    fs::write(dir.join("a.json"), r#"{"name":"alpha"}"#).unwrap();
    fs::write(dir.join("b.json"), r#"{"name":"beta"}"#).unwrap();
    fs::write(dir.join("bad.json"), r#"{"invalid":true}"#).unwrap();
    fs::write(dir.join("note.txt"), "NOTE_SENTINEL_é\nCheck edge cases.\n").unwrap();
    let mut args: Vec<String> = [
        "--experiment",
        "variants",
        "--model",
        "codex/mock",
        "--unregistered",
        "--max-invocations",
        "100",
        "--max-wall-clock-secs",
        "600",
        "--k",
        "2",
        "--primary",
        "D",
        "--min-families",
        "1",
        "--seed",
        "7",
        "--arms",
        arms,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    args.extend([
        "--suite".to_string(),
        dir.join("suite").display().to_string(),
        "--out".to_string(),
        dir.join("run").display().to_string(),
    ]);
    args
}

fn add_variant(args: &mut Vec<String>, flag: &str, name: &str, path: &Path) {
    args.extend([flag.to_string(), format!("{name}={}", path.display())]);
}

fn run_mock(args: &[String], runner: &dyn ArmC, provider: Arc<dyn Provider>) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    crate::cli::run_with_provider(args, Some(runner), &root, Some(provider))
}

#[test]
fn named_variants_route_harnesses_notes_budgets_and_disk_paths() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "C:a,C:b,D,D:lessons");
    add_variant(&mut args, "--harness", "a", &dir.path().join("a.json"));
    add_variant(&mut args, "--harness", "b", &dir.path().join("b.json"));
    add_variant(
        &mut args,
        "--d-note",
        "lessons",
        &dir.path().join("note.txt"),
    );
    args.push("--record-hidden-output".to_string());
    let runner = HarnessMock::default();
    let (provider, _) = mock(|r, _| {
        if r.prompt.contains("Previous attempt") {
            GOOD
        } else {
            WRONG
        }
        .to_string()
    });
    run_mock(&args, &runner, provider).unwrap();
    let received = runner.seen.lock().unwrap();
    assert_eq!(received.len(), 2);
    for name in ["alpha", "beta"] {
        assert!(received.contains(&(json!({"name": name}), 2)));
    }
    let records = ledger_records(&dir.path().join("run"));
    let registration = &records[0];
    for name in ["a", "b"] {
        let bytes = fs::read(dir.path().join(format!("{name}.json"))).unwrap();
        assert_eq!(
            registration["harnesses"][name]["harness"],
            serde_json::from_slice::<Value>(&bytes).unwrap()
        );
        assert_eq!(
            registration["harnesses"][name]["harness_sha256"],
            ledger::sha256_hex(&bytes)
        );
    }
    let note = fs::read(dir.path().join("note.txt")).unwrap();
    assert_eq!(
        registration["d_notes"]["lessons"]["sha256"],
        ledger::sha256_hex(&note)
    );
    assert_eq!(registration["d_notes"]["lessons"]["bytes"], note.len());
    for label in ["C:a", "C:b", "D", "D:lessons"] {
        let submission = records
            .iter()
            .find(|r| r["kind"] == "submission" && r["arm"] == label)
            .unwrap();
        assert_eq!(submission["invocations"], 2);
        let disk = label.replace(':', "-");
        assert!(
            dir.path()
                .join(format!("run/submissions/{disk}/t1/r0.rs"))
                .is_file()
        );
        assert!(
            dir.path()
                .join(format!("run/submissions/{disk}/t1/r0.hidden.txt"))
                .is_file()
        );
        assert!(
            dir.path()
                .join(format!("run/invocations/{disk}/t1/r0"))
                .is_dir()
        );
        let calls: Vec<_> = records
            .iter()
            .filter(|r| r["kind"] == "invocation" && r["arm"] == label)
            .collect();
        assert_eq!(calls.len(), 2);
    }
    for label in ["D", "D-lessons"] {
        let prompts: Vec<_> = walk(&dir.path().join(format!("run/invocations/{label}")))
            .into_iter()
            .filter(|p| p.to_string_lossy().contains("prompt"))
            .collect();
        assert_eq!(prompts.len(), 2);
        for path in prompts {
            let prompt = fs::read_to_string(path).unwrap();
            assert_eq!(prompt.contains("NOTE_SENTINEL_é"), label == "D-lessons");
            if label == "D-lessons" {
                assert!(
                    prompt.contains("\n```\n\n\nNOTE_SENTINEL_é\nCheck edge cases.\n"),
                    "{prompt}"
                );
            }
        }
    }
    let summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run/summary.json")).unwrap()).unwrap();
    for (x, y) in [
        ("C:a", "C:b"),
        ("C:a", "D"),
        ("C:b", "D"),
        ("C:a", "D:lessons"),
    ] {
        assert!(
            summary["comparisons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["treatment"] == x && c["comparator"] == y)
        );
    }
    assert!(ledger::verify_chain(&dir.path().join("run/ledger.jsonl")).is_ok());
}

#[test]
fn variants_and_claims_are_validated_before_registration() {
    for (arms, definitions, claim, expected) in [
        ("D,C:a", vec![], None, "undefined"),
        ("D", vec![("--harness", "a", "a.json")], None, "not listed"),
        ("D,D:lessons", vec![], None, "undefined"),
        (
            "D",
            vec![("--d-note", "lessons", "note.txt")],
            None,
            "not listed",
        ),
        (
            "D,C:a,C:b",
            vec![("--harness", "a", "a.json"), ("--harness", "b", "bad.json")],
            None,
            "invalid mock harness",
        ),
        (
            "D,C:a",
            vec![("--harness", "a", "a.json"), ("--harness", "a", "b.json")],
            None,
            "twice",
        ),
        (
            "D,C:a",
            vec![("--harness", "a", "a.json")],
            Some("D>C:a"),
            "computed comparison",
        ),
        (
            "D,C:a",
            vec![("--harness", "a", "a.json")],
            Some("C:a>C:missing"),
            "computed comparison",
        ),
        (
            "D,C:a",
            vec![("--harness", "a", "a.json")],
            Some(""),
            "claim",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut args = variant_args(dir.path(), arms);
        for (flag, name, path) in definitions {
            add_variant(&mut args, flag, name, &dir.path().join(path));
        }
        if let Some(claim) = claim {
            args.extend(["--claim".to_string(), claim.to_string()]);
        }
        let (provider, seen) = mock(|_, _| GOOD.to_string());
        let error = run_mock(&args, &HarnessMock::default(), provider).unwrap_err();
        assert!(error.contains(expected), "{error} (expected {expected})");
        assert!(!dir.path().join("run/ledger.jsonl").exists());
        assert!(seen.lock().unwrap().is_empty());
    }
}

#[test]
fn multi_comparison_claim_requires_all_pairs_and_uses_first_for_falsification() {
    let a = Arm::parse("C:a").unwrap();
    let b = Arm::parse("C:b").unwrap();
    let mut plan = claim_plan();
    plan.arms = vec![a.clone(), b.clone(), Arm::D];
    plan.claim = Some(vec![(a.clone(), b.clone()), (a.clone(), Arm::D)]);
    let result = |a_pass: bool, b_pass: bool, d_pass: bool| RunResult {
        outcomes: (0..12)
            .flat_map(|f| {
                [
                    outcome(a.clone(), f, 0, a_pass),
                    outcome(b.clone(), f, 0, b_pass),
                    outcome(Arm::D, f, 0, d_pass),
                ]
            })
            .collect(),
        tuples_planned: 12,
        aborted: false,
        hidden_name_hits: Vec::new(),
        scored: true,
    };
    let supported = summarize(&result(true, false, false), &plan);
    // An integrity failure dominates even when every pair would be Supported.
    let mut flagged = result(true, false, false);
    flagged.hidden_name_hits = vec!["h_secret".to_string()];
    assert_eq!(summarize(&flagged, &plan)["claim"], "IntegrityReview");
    assert_eq!(supported["claim_rule"], "all-supported");
    assert_eq!(supported["claim"], "SUPPORTED");
    assert_eq!(supported["claim_comparisons"][0]["class"], "Supported");
    assert_eq!(
        summarize(&result(true, false, true), &plan)["claim"],
        "INCONCLUSIVE"
    );
    assert_eq!(
        summarize(&result(false, true, false), &plan)["claim"],
        "RULED OUT"
    );
    assert_eq!(
        summarize(&result(true, true, false), &plan)["claim"],
        "INCONCLUSIVE"
    );
    plan.min_families = 13;
    assert_eq!(
        summarize(&result(true, false, false), &plan)["claim"],
        "INCONCLUSIVE"
    );
    plan.min_families = 10;
    let mut invalid = result(true, false, false);
    invalid.outcomes[0].tool_calls = 1;
    assert_eq!(summarize(&invalid, &plan)["claim"], "IntegrityReview");
    invalid.outcomes[0].tool_calls = 0;
    invalid.tuples_planned = 20;
    assert_eq!(summarize(&invalid, &plan)["claim"], "Invalid");
}

#[test]
fn isolated_tool_event_voids_its_tuple_but_a_high_rate_is_integrity_review() {
    // 70 one-task families x 3 arms = 210 invocations in one replicate.
    let a = Arm::parse("C:a").unwrap();
    let mut plan = claim_plan();
    plan.arms = vec![a.clone(), Arm::B, Arm::D];
    plan.claim = Some(vec![(a.clone(), Arm::D), (a.clone(), Arm::B)]);
    let run = |events: usize| {
        let mut result = RunResult {
            outcomes: (0..70)
                .flat_map(|f| {
                    [
                        outcome(a.clone(), f, 0, true),
                        outcome(Arm::B, f, 0, f % 2 == 0),
                        outcome(Arm::D, f, 0, f % 2 == 0),
                    ]
                })
                .collect(),
            tuples_planned: 70,
            aborted: false,
            hidden_name_hits: Vec::new(),
            scored: true,
        };
        for i in 0..events {
            result.outcomes[3 * i].tool_calls = 1;
        }
        result
    };
    // One event: 1/210 = 0.48% <= 0.5%; the tuple is void and excluded.
    let one = summarize(&run(1), &plan);
    assert_eq!(one["integrity_ok"], true);
    assert_eq!(one["tool_event_tuples"], 1);
    assert_eq!(one["tuples_void"], 1);
    assert_eq!(one["tuples_complete"], 69);
    assert_ne!(one["claim"], "IntegrityReview");
    // Two events: 2/210 = 0.95% > 0.5%: the whole run is IntegrityReview.
    let two = summarize(&run(2), &plan);
    assert_eq!(two["integrity_ok"], false);
    assert_eq!(two["claim"], "IntegrityReview");
}

#[test]
fn variant_registration_survives_manifest_scoring_pooling_and_compare() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "C:a,C:b,D,D:lessons");
    add_variant(&mut args, "--harness", "a", &dir.path().join("a.json"));
    add_variant(&mut args, "--harness", "b", &dir.path().join("b.json"));
    add_variant(
        &mut args,
        "--d-note",
        "lessons",
        &dir.path().join("note.txt"),
    );
    let primary = args.iter().position(|a| a == "--primary").unwrap();
    args.drain(primary..primary + 2);
    args.extend(["--replicates".to_string(), "2".to_string()]);
    args.extend([
        "--defer-scoring".to_string(),
        "--claim".to_string(),
        "C:a>C:b,C:a>D".to_string(),
    ]);
    let (provider, _) = mock(|_, _| GOOD.to_string());
    run_mock(&args, &HarnessMock::default(), provider).unwrap();
    let run = dir.path().join("run");
    assert_eq!(
        cli_main(&vec!["manifest".into(), run.display().to_string()], None),
        0
    );
    let (result, plan, _) =
        score_deferred(&run, &dir.path().join("suite"), 1, false, None).unwrap();
    assert_eq!(result.outcomes.len(), 8);
    assert!(result.outcomes.iter().all(|o| o.hidden.all_passed));
    assert_eq!(plan.claim.as_ref().unwrap().len(), 2);
    assert_eq!(plan.harnesses["b"], json!({"name":"beta"}));
    let ledger = run.join("ledger.jsonl");
    let (pooled, pooled_plan) = pool_ledgers(&[ledger.clone(), ledger.clone()]).unwrap();
    assert_eq!(pooled.outcomes.len(), 16);
    assert_eq!(pooled_plan.replicates, 4);
    assert_eq!(pooled.tuples_planned, 4);
    assert_eq!(summarize(&pooled, &pooled_plan)["complete"], true);
    assert_eq!(
        summarize(&pooled, &pooled_plan)["claim_rule"],
        "all-supported"
    );
    for key in ["harnesses", "d_notes", "claim"] {
        let path = dir.path().join(format!("changed-{key}.jsonl"));
        let mut changed = Ledger::create(&path).unwrap();
        let mut registration = ledger_records(&run)[0].clone();
        registration[key] = json!(null);
        changed.append("registration", registration).unwrap();
        let error = pool_ledgers(&[ledger.clone(), path]).unwrap_err();
        assert!(error.contains(&format!("'{key}' differs")), "{error}");
    }
    let out = dir.path().join("comparison.json");
    assert_eq!(
        cli_main(
            &vec![
                "compare".into(),
                "--out".into(),
                out.display().to_string(),
                format!("{}:C:a", ledger.display()),
                format!("{}:C:b", ledger.display())
            ],
            None
        ),
        0
    );
    let comparison: Value = serde_json::from_slice(&fs::read(out).unwrap()).unwrap();
    assert_eq!(comparison["x"]["arm"], "C:a");
    assert_eq!(comparison["x"]["harness"], json!({"name":"alpha"}));
    assert_eq!(comparison["y"]["arm"], "C:b");
    assert_eq!(comparison["n"], 2);
}

#[test]
fn plain_harness_registration_and_legacy_claim_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "C,D");
    args.extend([
        "--harness".to_string(),
        dir.path().join("a.json").display().to_string(),
    ]);
    let (provider, _) = mock(|_, _| GOOD.to_string());
    let runner = HarnessMock::default();
    run_mock(&args, &runner, provider).unwrap();
    assert_eq!(runner.seen.lock().unwrap()[0].0, json!({"name":"alpha"}));
    let records = ledger_records(&dir.path().join("run"));
    assert_eq!(records[0]["harness"], json!({"name":"alpha"}));
    assert_eq!(
        records[0]["harness_sha256"],
        ledger::sha256_hex(&fs::read(dir.path().join("a.json")).unwrap())
    );
    let summary = records.iter().find(|r| r["kind"] == "summary").unwrap();
    assert!(summary.get("claim_rule").is_none());
    assert_eq!(summary["comparisons"][0]["role"], "primary");
}

#[test]
fn variant_orders_cover_all_arms_and_comparisons_keep_c_pair_direction() {
    let mut plan = claim_plan();
    plan.arms = ["C:b", "D:notes", "B", "C", "A", "D", "C:a"]
        .into_iter()
        .map(|s| Arm::parse(s).unwrap())
        .collect();
    let mut expected = plan.arms.clone();
    expected.sort();
    let orders: Vec<_> = (0..20)
        .map(|i| arm_order(&plan, &format!("task-{i}"), 1))
        .collect();
    for order in &orders {
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, expected);
    }
    assert_eq!(orders[3], arm_order(&plan, "task-3", 1));
    assert!(orders.iter().any(|o| o != &orders[0]));
    let pairs = comparison_pairs(&plan.arms);
    assert_eq!(pairs.len(), 18);
    let c_pairs: Vec<_> = pairs
        .iter()
        .filter(|(x, y)| x.is_c() && y.is_c())
        .map(|(x, y)| (x.label(), y.label()))
        .collect();
    assert_eq!(
        c_pairs,
        [("C:b", "C"), ("C:b", "C:a"), ("C", "C:a")].map(|(x, y)| (x.to_string(), y.to_string()))
    );
}

#[test]
fn named_c_variants_share_the_global_invocation_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = claim_plan();
    plan.arms = ["C:a", "C:b"]
        .into_iter()
        .map(|s| Arm::parse(s).unwrap())
        .collect();
    plan.harnesses = [("a".to_string(), json!({})), ("b".to_string(), json!({}))]
        .into_iter()
        .collect();
    plan.k = 2;
    let meter = meter_in(dir.path(), 3);
    let (provider, seen) = mock(|_, _| GOOD.to_string());
    let result = run_arms(
        &plan,
        &suite(),
        provider,
        &meter,
        dir.path(),
        Some(&HarnessMock::default()),
    )
    .unwrap();
    assert!(result.aborted);
    assert_eq!(result.outcomes.len(), 1);
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn pooling_accepts_legacy_registration_without_variant_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy.jsonl");
    let current = dir.path().join("current.jsonl");
    synthetic_ledger(&legacy, 7, 4);
    let mut ledger = Ledger::create(&current).unwrap();
    for line in fs::read_to_string(&legacy).unwrap().lines() {
        let mut record: Value = serde_json::from_str(line).unwrap();
        if record["kind"] == "registration" {
            record["harnesses"] = json!({});
            record["d_notes"] = json!({});
            record["claim"] = Value::Null;
        }
        let kind = record["kind"].as_str().unwrap().to_string();
        ledger.append(&kind, record).unwrap();
    }
    let (result, plan) = pool_ledgers(&[legacy, current]).unwrap();
    assert_eq!(result.outcomes.len(), 16);
    assert_eq!(plan.replicates, 2);
}

/// Record every controller-to-child line on stderr, including oracle feedback.
fn external_fixture(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("harness.py");
    fs::write(
        &path,
        format!(
            r#"import json, os, sys
assert sys.flags.isolated == 1
assert os.listdir('.') == []
# Apple's /usr/bin/python3 launcher adds toolchain variables after exec.
assert set(os.environ) <= {{'PATH', 'HOME', 'LANG', 'TMPDIR', 'LC_CTYPE', 'CPATH',
    'LIBRARY_PATH', 'MANPATH', 'SDKROOT', '__CF_USER_TEXT_ENCODING'}}
def receive():
    line = sys.stdin.readline()
    print(line.rstrip(), file=sys.stderr, flush=True)
    return json.loads(line)
def request(value):
    print(json.dumps(value), flush=True)
    return receive()
task = receive()['task']
{body}
"#
        ),
    )
    .unwrap();
    path
}

fn run_external_mock(args: &[String], provider: Arc<dyn Provider>) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = crate::cli::run_with_provider(args, None, &root, Some(provider));
    if result.is_ok() {
        let out = args.iter().position(|a| a == "--out").unwrap();
        let run = Path::new(&args[out + 1]);
        for record in ledger_records(run)
            .iter()
            .filter(|r| r["kind"] == "external_error")
        {
            eprintln!("{record}");
        }
        for path in walk(&run.join("invocations")) {
            if path
                .file_name()
                .is_some_and(|name| name == "harness.stderr.txt")
            {
                eprintln!("{}", fs::read_to_string(path).unwrap());
            }
        }
    }
    result
}

#[test]
fn external_round_trip_is_public_only_metered_and_logged() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "X:nano");
    let harness = external_fixture(
        dir.path(),
        r#"
assert task['id'] == 't1' and task['family'] == 'fam-a'
assert task['k'] == 2 and task['max_feedback_chars'] == 6000
answer = request({'op': 'model', 'system': task['coder_system'], 'prompt': task['task_prompt']})
assert answer['ok'], answer
code = request({'op': 'extract', 'text': answer['text']})['code']
assert code is not None
assert request({'op': 'extract', 'text': 'no code'}) == {'code': None}
score = request({'op': 'public_test', 'code': code})
assert score['compiled'] and score['all_passed'] and not score['timed_out'], score
assert (score['passed'], score['failed'], score['expected']) == (1, 0, 1)
assert 'test result:' in score['output']
assert request({'op': 'unknown'}) == {'ok': False, 'error': 'unknown op'}
print(json.dumps({'op': 'done', 'codes': [code]}), flush=True)
"#,
    );
    add_variant(&mut args, "--external", "nano", &harness);
    let (provider, seen) = mock(|r, _| {
        assert_eq!(r.enzyme_id.0, "lift-coder");
        assert_eq!(r.max_tokens, 32_000);
        GOOD.to_string()
    });
    run_external_mock(&args, provider).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
    let run = dir.path().join("run");
    let records = ledger_records(&run);
    assert_eq!(
        records[0]["externals"]["nano"]["path"],
        harness.display().to_string()
    );
    assert_eq!(
        records[0]["externals"]["nano"]["sha256"],
        ledger::sha256_hex(&fs::read(harness).unwrap())
    );
    let calls: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "invocation")
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["arm"], "X:nano");
    assert!(
        fs::read_to_string(calls[0]["prompt_path"].as_str().unwrap())
            .unwrap()
            .contains(arms::CODER_SYSTEM)
    );
    assert_eq!(
        fs::read_to_string(calls[0]["response_path"].as_str().unwrap()).unwrap(),
        GOOD
    );
    let traffic =
        fs::read_to_string(run.join("invocations/X-nano/t1/r0/harness.stderr.txt")).unwrap();
    assert!(!traffic.contains(SENTINEL), "{traffic}");
    let first: Value =
        serde_json::from_str(traffic.lines().find(|l| l.starts_with('{')).unwrap()).unwrap();
    let task = task::load_suite(&dir.path().join("suite"), None)
        .unwrap()
        .tasks[0]
        .public();
    assert_eq!(
        first,
        json!({"task": {"id": task.id, "family": task.family,
        "task_prompt": arms::task_prompt(&task), "coder_system": arms::CODER_SYSTEM,
        "k": 2, "max_feedback_chars": 6000}})
    );
    let submission = records.iter().find(|r| r["kind"] == "submission").unwrap();
    assert_eq!(submission["invocations"], 1);
    assert_eq!(submission["coder_calls"], 1);
    assert_eq!(submission["public"]["all_passed"], true);
    assert!(
        records
            .iter()
            .any(|r| r["kind"] == "score" && r["result"]["all_passed"] == true)
    );
    let summary = records.iter().find(|r| r["kind"] == "summary").unwrap();
    assert_eq!(summary["arms"][0]["arm"], "X:nano");
    ledger::verify_chain(&run.join("ledger.jsonl")).unwrap();
}

#[test]
fn external_lane_calls_share_the_budget_and_are_logged() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "X:nano");
    let k = args.iter().position(|a| a == "--k").unwrap();
    args[k + 1] = "2".to_string();
    let harness = external_fixture(
        dir.path(),
        r#"
coder = request({'op': 'model', 'system': 'coder', 'prompt': 'write'})
assert coder['ok'], coder
judge = request({'op': 'model', 'lane': 'judge', 'system': 'judge', 'prompt': 'pick'})
assert judge['ok'], judge
third = request({'op': 'model', 'lane': 'judge', 'system': 'judge', 'prompt': 'again'})
assert not third['ok'] and 'budget exhausted' in third['error'], third
missing = request({'op': 'model', 'lane': 'nope', 'system': 's', 'prompt': 'p'})
assert not missing['ok'] and 'unknown lane' in missing['error'], missing
print(json.dumps({'op': 'done', 'codes': []}), flush=True)
"#,
    );
    add_variant(&mut args, "--external", "nano", &harness);
    args.push("--lane".into());
    args.push("judge=codex/gpt-6.1-sol:medium".into());
    let (provider, seen) = mock(|_, _| GOOD.to_string());
    run_external_mock(&args, provider).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    let records = ledger_records(&dir.path().join("run"));
    let enzymes: Vec<&str> = records
        .iter()
        .filter(|r| r["kind"] == "invocation")
        .map(|r| r["enzyme"].as_str().unwrap())
        .collect();
    assert_eq!(enzymes, vec!["lift-coder", "lane:judge"]);
    let registration = records.iter().find(|r| r["kind"] == "registration").unwrap();
    assert_eq!(registration["lanes"]["judge"], "codex/gpt-6.1-sol:medium");
}

#[test]
fn nano_harness_v0_matches_native_d_prompts_and_submission() {
    // Cycle-2 parity gate: the external nanoharness v0 (archived at
    // nano/history/before-E002b.py once E002 was promoted) must send exactly the
    // prompts native D sends, through both repair branches, and submit the
    // same file. Temp paths in public-test output are normalised.
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "D,X:v0");
    let k = args.iter().position(|a| a == "--k").unwrap();
    args[k + 1] = "3".to_string();
    let nano = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nano/history/before-E002b.py");
    add_variant(&mut args, "--external", "v0", &nano);
    let (provider, _) = mock(|request, _| {
        if request.prompt.contains("contained no ```rust code block") {
            WRONG.to_string()
        } else if request.prompt.contains("## Public test results") {
            GOOD.to_string()
        } else {
            "I would write it like this, but forgot the fence.".to_string()
        }
    });
    run_external_mock(&args, provider).unwrap();
    let run = dir.path().join("run");
    let normalise = |text: String| -> String {
        text.split_whitespace()
            // Per-call oracle randomness: temp paths, the randomised public
            // module name, and thread ids.
            .map(|w| {
                if w.contains("a2d-oracle-") || w.contains("/var/folders/") || w.contains("/tmp/") {
                    "<tmp>"
                } else if w.contains("a2d_public_") {
                    "<module>"
                } else if w.starts_with('(') && w.ends_with(')') && w[1..w.len() - 1].chars().all(|c| c.is_ascii_digit()) {
                    "(<thread>)"
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let prompts = |arm: &str| -> Vec<String> {
        let mut files: Vec<PathBuf> = walk(&run.join("invocations").join(arm))
            .into_iter()
            .filter(|p| p.to_string_lossy().ends_with(".prompt.txt"))
            .collect();
        files.sort();
        files.into_iter().map(|p| normalise(fs::read_to_string(p).unwrap())).collect()
    };
    let d = prompts("D");
    let x = prompts("X-v0");
    assert_eq!(d.len(), 3, "D should repair twice and stop at green");
    assert_eq!(d, x, "nanoharness v0 prompts differ from native D");
    let sub = |arm: &str| fs::read_to_string(run.join("submissions").join(arm).join("t1/r0.rs")).unwrap();
    assert_eq!(sub("D"), sub("X-v0"));
}

#[test]
fn external_model_calls_cannot_exceed_k() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "X:nano");
    let harness = external_fixture(
        dir.path(),
        r#"
for i in range(task['k'] + 1):
    answer = request({'op': 'model', 'system': 'system', 'prompt': 'prompt'})
    assert answer['ok'] == (i < task['k']), answer
assert 'budget exhausted' in answer['error']
print(json.dumps({'op': 'done', 'codes': []}), flush=True)
"#,
    );
    add_variant(&mut args, "--external", "nano", &harness);
    let (provider, seen) = mock(|_, _| GOOD.to_string());
    run_external_mock(&args, provider).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    let records = ledger_records(&dir.path().join("run"));
    assert_eq!(
        records.iter().filter(|r| r["kind"] == "invocation").count(),
        2
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r["kind"] == "invocation_refused")
            .count(),
        1
    );
    let traffic = fs::read_to_string(
        dir.path()
            .join("run/invocations/X-nano/t1/r0/harness.stderr.txt"),
    )
    .unwrap();
    let last: Value = serde_json::from_str(traffic.lines().last().unwrap()).unwrap();
    assert_eq!(last["ok"], false);
}

#[test]
fn external_done_codes_use_public_selection_and_latest_ties() {
    for public_tests in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let mut args = variant_args(dir.path(), "X:nano");
        if !public_tests {
            fs::remove_file(dir.path().join("suite/t1/public_tests.rs")).unwrap();
        }
        let codes = [
            "fn double(x: i32) -> i32 { x * 2 } // first",
            "fn double(x: i32) -> i32 { 2 * x } // latest green",
            "fn double(x: i32) -> i32 { x }",
            "fn double(",
        ];
        let harness = external_fixture(
            dir.path(),
            &format!(
                "os.close(sys.stdin.fileno())\nprint(json.dumps({{'op': 'done', 'codes': {}}}), flush=True)",
                json!(codes)
            ),
        );
        add_variant(&mut args, "--external", "nano", &harness);
        let (provider, seen) = mock(|_, _| panic!("no model call requested"));
        run_external_mock(&args, provider).unwrap();
        assert!(seen.lock().unwrap().is_empty());
        let records = ledger_records(&dir.path().join("run"));
        let submission = records.iter().find(|r| r["kind"] == "submission").unwrap();
        let index = if public_tests { 1 } else { 3 };
        assert_eq!(submission["attempts"], 4);
        assert_eq!(submission["attempt_index"], index);
        assert_eq!(
            fs::read_to_string(dir.path().join("run/submissions/X-nano/t1/r0.rs")).unwrap(),
            codes[index]
        );
    }
}

#[test]
fn external_exit_without_done_submits_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "X:nano");
    let harness = external_fixture(
        dir.path(),
        r#"
answer = request({'op': 'model', 'system': '', 'prompt': ''})
code = request({'op': 'extract', 'text': answer['text']})['code']
request({'op': 'public_test', 'code': code})
"#,
    );
    add_variant(&mut args, "--external", "nano", &harness);
    let (provider, _) = mock(|_, _| GOOD.to_string());
    run_external_mock(&args, provider).unwrap();
    let records = ledger_records(&dir.path().join("run"));
    let submission = records.iter().find(|r| r["kind"] == "submission").unwrap();
    assert_eq!(submission["invocations"], 1);
    assert_eq!(submission["attempts"], 0);
    assert!(submission["artifact_sha256"].is_null());
    assert!(submission["void_reason"].is_null());
    assert!(
        records
            .iter()
            .any(|r| r["kind"] == "score" && r["result"]["all_passed"] == false)
    );
}

#[test]
fn external_hang_is_killed_at_wall_clock_and_voids_tuple() {
    let dir = tempfile::tempdir().unwrap();
    let harness = external_fixture(dir.path(), "import time\ntime.sleep(30)");
    let mut plan = claim_plan();
    plan.arms = vec![Arm::parse("X:nano").unwrap()];
    plan.externals.insert("nano".into(), harness);
    let meter = Meter::new(
        Ledger::create(&dir.path().join("ledger.jsonl")).unwrap(),
        100,
        Duration::from_secs(1),
        dir.path().join("invocations"),
    );
    let mut suite = suite();
    suite.tasks.truncate(1);
    let (provider, _) = mock(|_, _| panic!("no model call requested"));
    let start = std::time::Instant::now();
    let result = run_arms(&plan, &suite, provider, &meter, dir.path(), None).unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));
    assert_eq!(
        result.outcomes[0].void_reason.as_deref(),
        Some("wall-clock cap reached")
    );
    let records = ledger_records(dir.path());
    let submission = records.iter().find(|r| r["kind"] == "submission").unwrap();
    assert_eq!(submission["void_reason"], "wall-clock cap reached");
}

#[test]
fn external_harness_cannot_read_files_outside_its_working_directory() {
    // A static guard on harness source can be bypassed (sys.modules reaches
    // io and os); the OS sandbox must stop reads of hidden tests, holdouts
    // and the oracle's temp dirs, while the harness itself still runs.
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("hidden_tests.rs");
    fs::write(&secret, "SECRET-HIDDEN-CONTENT").unwrap();
    let home = std::env::var("HOME").unwrap();
    let harness = external_fixture(
        dir.path(),
        &format!(
            "def probe(f):\n    try:\n        return 'read ' + str(f())[:40]\n    except Exception as e:\n        return 'blocked ' + type(e).__name__\n\
             notes = [probe(lambda: sys.modules['io'].open({secret}).read()),\n\
             \x20        probe(lambda: os.listdir({home})),\n\
             \x20        probe(lambda: sys.modules['socket'].create_connection(('127.0.0.1', 9)) if 'socket' in sys.modules else 'no socket'),\n\
             \x20        'spawn ' + str(sys.modules['os'].system('/bin/echo x'))]\n\
             print(json.dumps({{'op': 'done', 'codes': ['// ' + ' | '.join(notes) + '\\nfn double(x: i32) -> i32 {{ x * 2 }}']}}), flush=True)",
            secret = json!(secret.to_str().unwrap()),
            home = json!(home),
        ),
    );
    let mut plan = claim_plan();
    plan.arms = vec![Arm::parse("X:nano").unwrap()];
    plan.externals.insert("nano".into(), harness);
    let meter = Meter::new(
        Ledger::create(&dir.path().join("ledger.jsonl")).unwrap(),
        100,
        Duration::from_secs(60),
        dir.path().join("invocations"),
    );
    let mut suite = suite();
    suite.tasks.truncate(1);
    let (provider, _) = mock(|_, _| panic!("no model call requested"));
    let result = run_arms(&plan, &suite, provider, &meter, dir.path(), None).unwrap();
    assert_eq!(result.outcomes[0].void_reason, None);
    let submitted = fs::read_to_string(dir.path().join("submissions/X-nano/t1/r0.rs")).unwrap();
    assert!(!submitted.contains("SECRET"), "hidden content reached the harness: {submitted}");
    assert!(submitted.contains("blocked PermissionError | blocked PermissionError"), "{submitted}");
    assert!(
        submitted.contains("| spawn ") && !submitted.contains("spawn 0"),
        "the harness spawned a process: {submitted}"
    );
}

#[test]
fn external_names_must_be_defined_listed_unique_and_safe() {
    for (arms, definitions, expected) in [
        ("X:nano", vec![], "undefined"),
        ("D", vec!["nano"], "not listed"),
        ("X:nano", vec!["nano", "nano"], "twice"),
        ("X:Bad", vec![], "invalid variant name"),
        ("D", vec!["../bad"], "invalid variant name"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut args = variant_args(dir.path(), arms);
        let harness = external_fixture(dir.path(), "pass");
        for name in definitions {
            add_variant(&mut args, "--external", name, &harness);
        }
        let (provider, seen) = mock(|_, _| GOOD.to_string());
        let error = run_external_mock(&args, provider).unwrap_err();
        assert!(error.contains(expected), "{error} (expected {expected})");
        assert!(seen.lock().unwrap().is_empty());
        assert!(!dir.path().join("run/ledger.jsonl").exists());
    }
}

#[test]
fn external_registration_survives_manifest_scoring_pooling_and_compare() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = variant_args(dir.path(), "X:nano,C:b,D");
    let harness = external_fixture(
        dir.path(),
        &format!(
            "print(json.dumps({{'op': 'done', 'codes': [{}]}}), flush=True)",
            json!("fn double(x: i32) -> i32 { x * 2 }")
        ),
    );
    add_variant(&mut args, "--external", "nano", &harness);
    add_variant(&mut args, "--harness", "b", &dir.path().join("b.json"));
    args.extend([
        "--defer-scoring".into(),
        "--claim".into(),
        "X:nano>C:b,X:nano>D".into(),
    ]);
    let (provider, _) = mock(|_, _| GOOD.to_string());
    run_mock(&args, &HarnessMock::default(), provider).unwrap();
    let run = dir.path().join("run");
    assert_eq!(
        cli_main(&["manifest".into(), run.display().to_string()], None),
        0
    );
    let (result, plan, _) =
        score_deferred(&run, &dir.path().join("suite"), 1, false, None).unwrap();
    assert!(
        result.outcomes.iter().all(|o| o.hidden.all_passed),
        "{result:?}"
    );
    let pairs: Vec<_> = comparison_pairs(&plan.arms)
        .into_iter()
        .map(|(x, y)| (x.label(), y.label()))
        .collect();
    assert_eq!(
        pairs,
        [("X:nano", "D"), ("X:nano", "C:b"), ("C:b", "D")]
            .map(|(x, y)| (x.to_string(), y.to_string()))
    );
    let ledger = run.join("ledger.jsonl");
    let (pooled, pooled_plan) = pool_ledgers(&[ledger.clone(), ledger.clone()]).unwrap();
    assert_eq!(pooled.outcomes.len(), 6);
    assert_eq!(summarize(&pooled, &pooled_plan)["complete"], true);
    let changed = dir.path().join("changed.jsonl");
    let mut changed_ledger = Ledger::create(&changed).unwrap();
    let mut registration = ledger_records(&run)[0].clone();
    registration["externals"]["nano"]["sha256"] = json!("changed");
    changed_ledger.append("registration", registration).unwrap();
    assert!(
        pool_ledgers(&[ledger.clone(), changed])
            .unwrap_err()
            .contains("'externals' differs")
    );
    let out = dir.path().join("compare.json");
    assert_eq!(
        cli_main(
            &[
                "compare".into(),
                "--out".into(),
                out.display().to_string(),
                format!("{}:X:nano", ledger.display()),
                format!("{}:D", ledger.display())
            ],
            None
        ),
        0
    );
    let comparison: Value = serde_json::from_slice(&fs::read(out).unwrap()).unwrap();
    assert_eq!(comparison["x"]["arm"], "X:nano");
    assert_eq!(
        comparison["x"]["harness"],
        ledger_records(&run)[0]["externals"]["nano"]
    );
}
