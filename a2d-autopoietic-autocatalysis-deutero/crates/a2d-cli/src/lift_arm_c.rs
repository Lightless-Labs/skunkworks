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

use crate::{seed_germline, seed_initial_runtime_artifacts};
use a2d_core::benchmark::{BenchmarkSuite, extract_rust_code_pub};
use a2d_core::metabolism::{CycleReport, Metabolism};
use a2d_core::provider::{Provider, ProviderRegistry};
use a2d_core::types::ArtifactType;
use a2d_core::workcell::WorkcellOutcome;
use a2d_lift::ArmC;
use a2d_lift::task::PublicTask;
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

impl ArmC for MetabolismArm {
    fn run(&self, provider: Box<dyn Provider>, task: &PublicTask, budget: usize) -> Vec<String> {
        let registry = ProviderRegistry::new(provider);
        let mut metabolism = Metabolism::new(seed_germline(), registry)
            .with_benchmark(public_benchmark(task))
            .with_max_invocations_per_cycle(budget)
            .with_provider_failure_cooldown(Duration::ZERO)
            .without_max_cycle_wall_clock();
        seed_initial_runtime_artifacts(&mut metabolism, &a2d_lift::arms::task_prompt(task));

        let mut codes = Vec::new();
        // Each productive cycle spends at least one invocation, so `budget`
        // cycles always exhaust the budget.
        for _ in 0..budget {
            let report = metabolism.run_cycle();
            codes.extend(code_outputs(&report));
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
            seed: 0,
            primary: Some(Arm::B),
            min_families: 1,
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
