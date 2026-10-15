//! Arm C of `a2d lift`: the seed A²D metabolism under the controller's budget.
//!
//! - Seed germline, fresh per task; lineage is never read or written.
//! - One provider (the controller's metered coder lane) serves every enzyme.
//! - The metabolism's benchmark carries the task's public tests only.
//! - No project root: architect `SystemPatch` proposals are rejected, never
//!   applied to this tree.
//! - Provider cooldown is off and the per-cycle wall clock is lifted: with a
//!   single lane, a cooldown would stall the arm rather than reroute, and the
//!   controller already caps invocations and wall-clock.
//!
//! Every code artifact the metabolism produces is returned in order; the
//! controller selects the submission with the same public rule as B and D.
//!
//! Harness-as-data (P3/P4): a registered JSON harness may
//! - `drop_enzymes`: remove seed enzymes (RAF closure must hold), freeing
//!   their invocations for the remaining schedule;
//! - `coder_note`: append text to the task input every enzyme sees;
//! - `coder_prompt`: replace the coder enzyme's prompt template (it must
//!   keep the seed's `main()` clause: the metabolism's sandbox compiles the
//!   file as a program);
//! - `route_previous_code`: give the coder its previous file (a
//!   `previous_attempt` food artifact, updated after every cycle, as a coder
//!   catalyst). The seed coder never sees its own code (LOG S0).
//!
//! `{}` is the seed configuration. Unknown keys are rejected.

use crate::{seed_germline, seed_initial_runtime_artifacts};
use a2d_core::benchmark::{BenchmarkSuite, extract_rust_code_pub};
use a2d_core::germline::Germline;
use a2d_core::metabolism::{CycleReport, Metabolism};
use a2d_core::provider::{Provider, ProviderRegistry};
use a2d_core::types::{ArtifactType, EnzymeId};
use a2d_core::workcell::WorkcellOutcome;
use a2d_lift::ArmC;
use a2d_lift::task::PublicTask;
use serde_json::Value;
use std::time::Duration;

pub struct MetabolismArm;

/// The metabolism's internal benchmark: public tests in the module name the
/// benchmark strips from candidate code and appends.
pub fn public_benchmark(task: &PublicTask) -> BenchmarkSuite {
    let acceptance_test = (!task.public_tests.trim().is_empty()).then(|| {
        format!(
            "\n#[cfg(test)]\n#[allow(unused_imports, dead_code)]\nmod a2d_acceptance {{\n    use super::*;\n{}\n}}\n",
            task.public_tests
        )
    });
    BenchmarkSuite {
        name: task.id.clone(),
        cases: Vec::new(),
        acceptance_test,
        test_timeout_secs: task.test_timeout_secs,
    }
}

fn code_outputs(report: &CycleReport) -> Vec<String> {
    let code = ArtifactType::from("code");
    // Same extraction contract as arms A, B and D: no extractable code, no attempt.
    report
        .lineage
        .iter()
        .filter_map(|entry| entry.outputs.get(&code))
        .filter_map(|bytes| extract_rust_code_pub(&String::from_utf8_lossy(bytes)))
        .collect()
}

fn refused(report: &CycleReport) -> bool {
    report.lineage.iter().any(|entry| {
        matches!(&entry.outcome, WorkcellOutcome::Failed { error } if error.contains(a2d_lift::meter::REFUSAL_PREFIX))
    })
}

const HARNESS_KEYS: &[&str] = &[
    "name",
    "drop_enzymes",
    "coder_note",
    "coder_prompt",
    "route_previous_code",
];

fn previous_attempt() -> ArtifactType {
    ArtifactType::from("previous_attempt")
}

/// The seed germline with the harness's enzyme removals applied.
fn harness_germline(harness: &Value) -> Result<Germline, String> {
    if let Some(object) = harness.as_object() {
        if let Some(key) = object.keys().find(|k| !HARNESS_KEYS.contains(&k.as_str())) {
            return Err(format!("unknown harness key {key:?}"));
        }
    } else {
        return Err("harness must be a JSON object".to_string());
    }
    let mut germline = seed_germline();
    for id in harness["drop_enzymes"].as_array().into_iter().flatten() {
        let id = id.as_str().ok_or("drop_enzymes must be strings")?;
        germline
            .propose_remove(&EnzymeId::from(id))
            .map_err(|e| format!("cannot drop {id}: {e}"))?;
    }
    let route = harness["route_previous_code"].as_bool().unwrap_or(false);
    let prompt = harness["coder_prompt"].as_str();
    if harness.get("coder_prompt").is_some() && prompt.is_none() {
        return Err("coder_prompt must be a string".to_string());
    }
    // The metabolism's sandbox compiles the file as a program before running
    // tests; without this clause the first attempt of every task is a
    // spurious E0601 (LOG E08).
    if prompt.is_some_and(|p| !p.contains("main()")) {
        return Err("coder_prompt must ask for a main() function".to_string());
    }
    if route || prompt.is_some() {
        let mut enzymes: Vec<_> = germline.enzymes().into_iter().cloned().collect();
        let coder = enzymes
            .iter_mut()
            .find(|e| e.id.0 == "coder")
            .ok_or("harness needs a coder enzyme")?;
        if route {
            coder.catalysts.insert(previous_attempt());
        }
        if let Some(prompt) = prompt {
            coder.prompt_template = Some(prompt.to_string());
        }
        let mut food = germline.food().clone();
        if route {
            food.insert(previous_attempt());
        }
        germline = Germline::new(enzymes, food);
    }
    Ok(germline)
}

fn harness_input(task: &PublicTask, harness: &Value) -> String {
    let mut input = a2d_lift::arms::task_prompt(task);
    if let Some(note) = harness["coder_note"].as_str() {
        input.push_str("\n\n");
        input.push_str(note);
    }
    input
}

impl ArmC for MetabolismArm {
    fn validate_harness(&self, harness: &Value) -> Result<(), String> {
        harness_germline(harness).map(|_| ())
    }

    fn run(
        &self,
        provider: Box<dyn Provider>,
        task: &PublicTask,
        budget: usize,
        harness: &Value,
    ) -> Vec<String> {
        let germline = match harness_germline(harness) {
            Ok(germline) => germline,
            Err(error) => {
                // validate_harness runs before registration; reaching this is a bug.
                eprintln!("arm C harness error: {error}");
                return Vec::new();
            }
        };
        let registry = ProviderRegistry::new(provider);
        let mut metabolism = Metabolism::new(germline, registry)
            .with_benchmark(public_benchmark(task))
            .with_max_invocations_per_cycle(budget)
            .with_provider_failure_cooldown(Duration::ZERO)
            .without_max_cycle_wall_clock();
        seed_initial_runtime_artifacts(&mut metabolism, &harness_input(task, harness));

        let route = harness["route_previous_code"].as_bool().unwrap_or(false);
        let mut codes = Vec::new();
        // Each productive cycle spends at least one invocation, so `budget`
        // cycles always exhaust the budget.
        for _ in 0..budget {
            let report = metabolism.run_cycle();
            let produced = code_outputs(&report);
            if route && let Some(last) = produced.last() {
                metabolism.seed_artifact(previous_attempt(), last.clone().into_bytes());
            }
            codes.extend(produced);
            if report.invocations == 0 || refused(&report) {
                break;
            }
        }
        codes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2d_core::provider::{InvocationRequest, InvocationResponse, ProviderError, TokenUsage};
    use a2d_lift::Arm;
    use std::sync::{Arc, Mutex};

    const SENTINEL: &str = "HIDDEN_SENTINEL_4b9d";

    /// Answers coder enzymes with a correct file and every other enzyme with
    /// plain text; records every request.
    struct EnzymeMock {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl Provider for EnzymeMock {
        fn name(&self) -> &str {
            "mock/enzymes"
        }

        fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{}\n{}", request.system, request.prompt));
            let text = if request.enzyme_id.0.contains("coder") {
                "```rust\nfn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n```"
            } else {
                "No changes."
            };
            Ok(InvocationResponse {
                text: text.to_string(),
                raw_output: None,
                tool_events: Vec::new(),
                transcript: None,
                thinking: None,
                usage: TokenUsage::default(),
            })
        }
    }

    #[test]
    fn all_four_arms_run_end_to_end_and_hidden_tests_never_reach_the_metabolism() {
        let suite_dir = tempfile::tempdir().unwrap();
        let task_dir = suite_dir.path().join("t1");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(
            task_dir.join("task.json"),
            r#"{"id":"t1","family":"fam","requirements":"Implement `fn double(x: i32) -> i32`."}"#,
        )
        .unwrap();
        std::fs::write(
            task_dir.join("public_tests.rs"),
            "#[test]\nfn doubles_two() { assert_eq!(double(2), 4); }\n",
        )
        .unwrap();
        std::fs::write(
            task_dir.join("hidden_tests.rs"),
            format!("#[test]\nfn doubles_negative() {{\n    // {SENTINEL}\n    assert_eq!(double(-3), -6);\n}}\n"),
        )
        .unwrap();
        let suite = a2d_lift::task::load_suite(suite_dir.path(), None).unwrap();

        let out = tempfile::tempdir().unwrap();
        let ledger = a2d_lift::ledger::Ledger::create(&out.path().join("ledger.jsonl")).unwrap();
        let meter = a2d_lift::meter::Meter::new(
            ledger,
            100,
            Duration::from_secs(600),
            out.path().join("invocations"),
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn Provider> = Arc::new(EnzymeMock { seen: seen.clone() });
        let plan = a2d_lift::RunPlan {
            arms: vec![Arm::A, Arm::B, Arm::D, Arm::C],
            k: 4,
            replicates: 1,
            jobs: 2,
            harness: serde_json::json!({}),
            seed: 0,
            primary: Some(Arm::B),
            min_families: 1,
            max_excluded_share: 0.0,
            record_hidden_output: false,
            defer_scoring: false,
        };
        let outcomes = a2d_lift::run_arms(
            &plan,
            &suite,
            provider,
            &meter,
            out.path(),
            Some(&MetabolismArm),
        )
        .unwrap()
        .outcomes;

        assert_eq!(outcomes.len(), 4);
        for outcome in &outcomes {
            assert!(outcome.hidden.all_passed, "{:?} failed", outcome.arm);
            assert!(outcome.invocations <= 4);
        }
        let c = outcomes.iter().find(|o| o.arm == Arm::C).unwrap();
        assert!(c.invocations >= 1);
        let seen = seen.lock().unwrap();
        assert!(seen.iter().any(|request| request.contains("doubles_two")));
        for request in seen.iter() {
            assert!(
                !request.contains(SENTINEL),
                "hidden test reached an enzyme:\n{request}"
            );
            assert!(!request.contains("doubles_negative"));
        }
    }

    /// Coders get a correct file; architect and tester outputs carry unique
    /// markers; every request is recorded with its enzyme id.
    struct MarkerMock {
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Provider for MarkerMock {
        fn name(&self) -> &str {
            "mock/markers"
        }

        fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
            self.seen.lock().unwrap().push((
                request.enzyme_id.0.clone(),
                format!("{}\n{}", request.system, request.prompt),
            ));
            let text = match request.enzyme_id.0.as_str() {
                "coder" => {
                    "```rust\nfn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n```".to_string()
                }
                "architect" => "ARCHITECT_OUTPUT_MARKER_7f3a".to_string(),
                "tester" => "TESTER_OUTPUT_MARKER_91c2".to_string(),
                _ => "No changes.".to_string(),
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

    #[test]
    fn architect_and_tester_outputs_never_reach_a_coder_prompt() {
        let task = PublicTask {
            id: "t".into(),
            family: "f".into(),
            requirements: "Implement `fn double(x: i32) -> i32`.".into(),
            public_tests: "#[test]\nfn doubles_two() { assert_eq!(double(2), 4); }\n".into(),
            test_timeout_secs: 10,
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let provider = Box::new(MarkerMock { seen: seen.clone() });
        MetabolismArm.run(provider, &task, 9, &serde_json::json!({}));
        let seen = seen.lock().unwrap();
        let enzymes: Vec<&str> = seen.iter().map(|(e, _)| e.as_str()).collect();
        assert!(
            enzymes.contains(&"architect") && enzymes.contains(&"tester"),
            "{enzymes:?}"
        );
        assert!(
            enzymes.iter().filter(|e| **e == "coder").count() >= 2,
            "{enzymes:?}"
        );
        for (enzyme, prompt) in seen.iter() {
            if enzyme == "coder" {
                assert!(
                    !prompt.contains("ARCHITECT_OUTPUT_MARKER_7f3a"),
                    "architect output reached the coder"
                );
                assert!(
                    !prompt.contains("TESTER_OUTPUT_MARKER_91c2"),
                    "tester output reached the coder"
                );
            }
        }
    }

    #[test]
    fn routed_harness_shows_the_coder_its_previous_attempt() {
        let task = PublicTask {
            id: "t".into(),
            family: "f".into(),
            requirements: "Implement `fn double(x: i32) -> i32`.".into(),
            public_tests: "#[test]\nfn doubles_two() { assert_eq!(double(2), 4); }\n".into(),
            test_timeout_secs: 10,
        };
        let h0 = serde_json::json!({
            "name": "h0", "drop_enzymes": ["architect", "tester"],
            "route_previous_code": true, "coder_prompt": "CUSTOM_CODER_PROMPT_e1 Include fn main(). Output a ```rust block."
        });
        let seen = Arc::new(Mutex::new(Vec::new()));
        MetabolismArm.run(Box::new(MarkerMock { seen: seen.clone() }), &task, 5, &h0);
        let seen = seen.lock().unwrap();
        let coder: Vec<&String> = seen
            .iter()
            .filter(|(e, _)| e == "coder")
            .map(|(_, p)| p)
            .collect();
        assert!(
            coder.len() >= 2,
            "{:?}",
            seen.iter().map(|(e, _)| e).collect::<Vec<_>>()
        );
        assert!(coder[0].contains("CUSTOM_CODER_PROMPT_e1"));
        assert!(
            !coder[0].contains("x * 2"),
            "first call has no previous attempt"
        );
        assert!(
            coder[1].contains("x * 2"),
            "second call must see the first attempt"
        );
        assert!(seen.iter().all(|(e, _)| e != "architect" && e != "tester"));
    }

    #[test]
    fn harness_drops_inert_enzymes_and_rejects_unknown_keys() {
        let minimal =
            serde_json::json!({"name": "minimal", "drop_enzymes": ["architect", "tester"]});
        let germline = harness_germline(&minimal).unwrap();
        let ids: Vec<String> = germline.enzymes().iter().map(|e| e.id.0.clone()).collect();
        assert!(!ids.contains(&"architect".to_string()) && !ids.contains(&"tester".to_string()));
        assert!(ids.contains(&"coder".to_string()));
        assert!(harness_germline(&serde_json::json!({"drop": []})).is_err());
        assert!(harness_germline(&serde_json::json!({"drop_enzymes": ["nope"]})).is_err());
        assert!(harness_germline(&serde_json::json!({})).is_ok());
    }

    #[test]
    fn coder_prompt_must_keep_the_sandbox_main_contract() {
        // The metabolism's sandbox compiles the file as a program before it
        // runs tests. A prompt that drops the seed's `main()` clause costs
        // every task a spurious E0601 attempt (E08 aborted).
        let no_main = serde_json::json!({"coder_prompt": "Write the file."});
        assert!(harness_germline(&no_main).unwrap_err().contains("main()"));
        let with_main =
            serde_json::json!({"coder_prompt": "Write the file. Include an empty `fn main() {}`."});
        assert!(harness_germline(&with_main).is_ok());
        let h0: Value =
            serde_json::from_str(include_str!("../../../harnesses/h0.json")).unwrap();
        assert!(harness_germline(&h0).is_ok());
    }

    #[test]
    fn metabolism_benchmark_requires_a_main_function() {
        let task = PublicTask {
            id: "t".into(),
            family: "f".into(),
            requirements: "Do it.".into(),
            public_tests: "#[test]\nfn doubles_two() { assert_eq!(double(2), 4); }\n".into(),
            test_timeout_secs: 10,
        };
        let suite = public_benchmark(&task);
        let library = suite.evaluate("```rust\npub fn double(x: i32) -> i32 { x * 2 }\n```");
        assert!(library.diagnostic.unwrap_or_default().contains("E0601"));
        let program = suite
            .evaluate("```rust\npub fn double(x: i32) -> i32 { x * 2 }\nfn main() {}\n```");
        assert_eq!(program.fitness, 1.0);
    }

    #[test]
    fn public_benchmark_carries_public_tests_only() {
        let task = PublicTask {
            id: "t".into(),
            family: "f".into(),
            requirements: "Do it.".into(),
            public_tests: "#[test]\nfn visible() {}\n".into(),
            test_timeout_secs: 7,
        };
        let suite = public_benchmark(&task);
        let acceptance = suite.acceptance_test.unwrap();
        assert!(acceptance.contains("mod a2d_acceptance"));
        assert!(acceptance.contains("fn visible()"));
        assert_eq!(suite.test_timeout_secs, 7);
        let empty = PublicTask {
            public_tests: String::new(),
            ..task
        };
        assert!(public_benchmark(&empty).acceptance_test.is_none());
    }
}
