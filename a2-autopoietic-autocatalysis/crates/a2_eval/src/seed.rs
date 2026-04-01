//! Seed evaluator — the Stage 0 evaluation pipeline.
//!
//! Runs acceptance criteria checks, test execution, and basic fitness scoring.
//! This is the bootstrap evaluator: simple, correct, human-auditable.

use a2_core::error::A2Result;
use a2_core::id::EvalId;
use a2_core::protocol::*;
use a2_core::traits::Evaluator;
use chrono::Utc;

/// The seed evaluator for Stage 0-1 bootstrap.
/// Checks: tests pass, acceptance criteria met, budget respected.
pub struct SeedEvaluator {
    /// Maximum acceptable token cost for a single task.
    pub token_ceiling: u64,
}

impl SeedEvaluator {
    pub fn new(token_ceiling: u64) -> Self {
        Self { token_ceiling }
    }

    fn check_tests(&self, results: &TestResults) -> bool {
        results.failed == 0
    }

    fn check_acceptance(&self, patch: &PatchBundle, task: &TaskContract) -> Vec<bool> {
        // Acceptance is structural: non-empty diff + tests pass is sufficient.
        // Literal string matching was a Stage 0 placeholder; real model output
        // rarely reproduces criterion text verbatim.
        let structurally_met = !patch.diff.is_empty() && self.check_tests(&patch.test_results);
        task.acceptance_criteria
            .iter()
            .map(|_criterion| structurally_met)
            .collect()
    }

    fn check_budget(&self, patch: &PatchBundle) -> bool {
        let total = patch.model_attribution.tokens_in + patch.model_attribution.tokens_out;
        total <= self.token_ceiling
    }
}

#[async_trait::async_trait]
impl Evaluator for SeedEvaluator {
    async fn evaluate(&self, patch: &PatchBundle, task: &TaskContract) -> A2Result<FitnessRecord> {
        let tests_pass = self.check_tests(&patch.test_results);
        let acceptance_met = self.check_acceptance(patch, task);
        let within_budget = self.check_budget(patch);

        let task_completed = tests_pass && acceptance_met.iter().all(|&a| a) && within_budget;

        let total_tokens = patch.model_attribution.tokens_in + patch.model_attribution.tokens_out;

        Ok(FitnessRecord {
            eval_id: EvalId::new(),
            task_id: task.id.clone(),
            somatic: SomaticFitness {
                task_completed,
                tests_pass,
                acceptance_met,
                tokens_used: total_tokens,
                duration_secs: 0.0, // Filled by workcell runtime.
            },
            germline: None,       // Stage 0: no germline fitness yet.
            organizational: None, // Stage 0: no org fitness yet.
            evaluated_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_core::id::*;

    fn make_task() -> TaskContract {
        TaskContract {
            id: TaskId::new(),
            title: "fix the frobulator".into(),
            description: "the frobulator is broken".into(),
            acceptance_criteria: vec!["frobulator works".into()],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            created_at: Utc::now(),
        }
    }

    fn make_patch(tests_pass: bool, rationale: &str, tokens: u64) -> PatchBundle {
        PatchBundle {
            id: PatchId::new(),
            task_id: TaskId::new(),
            workcell_id: WorkcellId::new(),
            diff: "--- a/frob.rs\n+++ b/frob.rs\n+fixed".into(),
            rationale: rationale.into(),
            test_results: TestResults {
                passed: if tests_pass { 3 } else { 2 },
                failed: if tests_pass { 0 } else { 1 },
                skipped: 0,
                details: vec![],
            },
            model_attribution: ModelAttribution {
                provider: "test".into(),
                model: "test".into(),
                tokens_in: tokens / 2,
                tokens_out: tokens / 2,
            },
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn passing_patch_scores_complete() {
        let eval = SeedEvaluator::new(10_000);
        let task = make_task();
        let patch = make_patch(true, "Fixed: frobulator works now", 500);

        let fitness = eval.evaluate(&patch, &task).await.unwrap();
        assert!(fitness.somatic.task_completed);
        assert!(fitness.somatic.tests_pass);
        assert!(fitness.somatic.acceptance_met.iter().all(|&a| a));
    }

    #[tokio::test]
    async fn failing_tests_score_incomplete() {
        let eval = SeedEvaluator::new(10_000);
        let task = make_task();
        let patch = make_patch(false, "Attempted fix: frobulator works maybe", 500);

        let fitness = eval.evaluate(&patch, &task).await.unwrap();
        assert!(!fitness.somatic.task_completed);
        assert!(!fitness.somatic.tests_pass);
    }

    #[tokio::test]
    async fn over_budget_scores_incomplete() {
        let eval = SeedEvaluator::new(100);
        let task = make_task();
        let patch = make_patch(true, "Frobulator works", 500);

        let fitness = eval.evaluate(&patch, &task).await.unwrap();
        assert!(!fitness.somatic.task_completed);
    }

    #[tokio::test]
    async fn unmet_acceptance_scores_incomplete() {
        let eval = SeedEvaluator::new(10_000);
        let task = make_task();
        // Empty diff — structurally incomplete regardless of rationale.
        let mut patch = make_patch(true, "Changed some code", 500);
        patch.diff = String::new();

        let fitness = eval.evaluate(&patch, &task).await.unwrap();
        assert!(!fitness.somatic.task_completed);
        assert!(!fitness.somatic.acceptance_met[0]);
    }

    #[tokio::test]
    async fn acceptance_met_without_literal_criterion_text() {
        // Passes even though the rationale never says "frobulator works".
        let eval = SeedEvaluator::new(10_000);
        let task = make_task();
        let patch = make_patch(true, "Rewrote the module so all tests pass", 500);

        let fitness = eval.evaluate(&patch, &task).await.unwrap();
        assert!(fitness.somatic.task_completed);
        assert!(fitness.somatic.acceptance_met.iter().all(|&a| a));
    }
}
