//! Governor — the A² control plane.
//!
//! Orchestrates the full task lifecycle: ingest → schedule → execute → evaluate → promote.
//! Five sub-components per DESIGN.md Section 3.7: Scheduler, Selector, Promoter, Analyst, Strategist.

use a2_core::error::A2Result;
use a2_core::id::*;
use a2_core::protocol::*;
use a2_core::traits::*;
use a2_workcell::runtime::{WorkcellConfig, WorkcellResult, run_workcell};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoundOutcome {
    test_count: u64,
    successful_applies: u64,
    promotion_count: u64,
}

/// Stage 0 detector for repeated non-improving rounds.
pub struct StagnationDetector {
    rounds: VecDeque<RoundOutcome>,
    capacity: usize,
}

impl StagnationDetector {
    pub fn new(capacity: usize) -> Self {
        Self {
            rounds: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    pub fn record_round(&mut self, test_count: u64, successful_applies: u64, promotion_count: u64) {
        if self.rounds.len() == self.capacity {
            self.rounds.pop_front();
        }

        self.rounds.push_back(RoundOutcome {
            test_count,
            successful_applies,
            promotion_count,
        });
    }

    pub fn is_stagnant(&self, window: usize) -> bool {
        if window == 0 || self.rounds.len() < window {
            return false;
        }

        let mut recent_rounds = self.rounds.iter().rev().take(window).collect::<Vec<_>>();
        recent_rounds.reverse();

        !recent_rounds.windows(2).any(|pair| {
            let previous = pair[0];
            let current = pair[1];

            current.test_count > previous.test_count
                || current.successful_applies > previous.successful_applies
                || current.promotion_count > previous.promotion_count
        })
    }

    pub fn trend(&self) -> f64 {
        if self.rounds.len() < 2 {
            return 0.0;
        }
        let deltas: Vec<f64> = self
            .rounds
            .iter()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|pair| pair[1].promotion_count as f64 - pair[0].promotion_count as f64)
            .collect();
        deltas.iter().sum::<f64>() / deltas.len() as f64
    }

    pub fn suggest_strategy_change(&self) -> String {
        "Recent rounds are flat. Try changing the model or catalyst strategy, or break the task into a smaller step.".into()
    }

    /// Update the last recorded round with the actual apply and verify outcomes.
    ///
    /// Called after the outer loop attempts `git apply` and `verify_and_rebuild`,
    /// replacing the provisional values set by `record_round` with ground truth.
    pub fn update_last_round_apply(&mut self, applied: bool, verified: bool) {
        if let Some(last) = self.rounds.back_mut() {
            last.successful_applies = u64::from(applied);
            last.promotion_count = u64::from(verified);
        }
    }

    fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Stage 0 Governor — minimal control plane for bootstrap.
///
/// In Stage 0, the governor is simple and linear:
/// 1. Accept task → 2. Create workcell → 3. Run catalyst → 4. Evaluate → 5. Decide promotion
///
/// All germline promotions require human approval (bootstrap profile B0).
pub struct Governor {
    germline_version: GermlineVersion,
    default_budget: Budget,
    stagnation_detector: Option<Mutex<StagnationDetector>>,
}

impl Governor {
    pub fn new(germline_version: GermlineVersion, default_budget: Budget) -> Self {
        Self {
            germline_version,
            default_budget,
            stagnation_detector: None,
        }
    }

    pub fn with_stagnation_detector(
        germline_version: GermlineVersion,
        default_budget: Budget,
        stagnation_detector: StagnationDetector,
    ) -> Self {
        Self {
            germline_version,
            default_budget,
            stagnation_detector: Some(Mutex::new(stagnation_detector)),
        }
    }

    /// Run a single task through the full pipeline.
    pub async fn run_task(
        &self,
        task: TaskContract,
        catalyst: &dyn Catalyst,
        model: &dyn ModelProvider,
        evaluator: &dyn Evaluator,
    ) -> A2Result<GovernorOutcome> {
        let workcell_id = WorkcellId::new();

        tracing::info!(
            workcell = %workcell_id,
            task = %task.id,
            "scheduling workcell"
        );

        // Stage 0 scheduler: single workcell, direct assignment.
        let config = WorkcellConfig {
            workcell_id: workcell_id.clone(),
            germline_version: self.germline_version.clone(),
            task: task.clone(),
            budget: self.default_budget.clone(),
        };

        // Execute.
        let result = run_workcell(config, catalyst, model, evaluator).await?;

        tracing::info!(
            workcell = %workcell_id,
            tokens = result.tokens_used,
            duration = format!("{:.1}s", result.duration_secs),
            patch = result.patch.is_some(),
            "workcell complete"
        );

        // Stage 0 promoter: always requires human approval.
        let decision = self.stage0_promote(&result);

        // Build lineage.
        let lineage = result.lineage.clone();

        self.record_round_outcome(&result, &decision);

        Ok(GovernorOutcome {
            workcell_id,
            task_id: task.id,
            result,
            decision,
            lineage,
        })
    }

    /// Stage 0 promotion: if tests pass and fitness looks good, flag for human review.
    /// Otherwise discard.
    fn stage0_promote(&self, result: &WorkcellResult) -> PromotionDecision {
        match &result.fitness {
            Some(f) if f.somatic.task_completed => {
                // B0: all germline mutations require human review.
                PromotionDecision::PromoteGermline {
                    mutation_scope: MutationScope::Prompt,
                }
            }
            _ => PromotionDecision::Discard {
                reason: "task not completed or evaluation failed".into(),
            },
        }
    }

    fn record_round_outcome(&self, result: &WorkcellResult, decision: &PromotionDecision) {
        let Some(detector) = &self.stagnation_detector else {
            return;
        };

        let test_count = result
            .patch
            .as_ref()
            .map(|patch| {
                u64::from(
                    patch.test_results.passed
                        + patch.test_results.failed
                        + patch.test_results.skipped,
                )
            })
            .unwrap_or(0);
        let successful_applies = u64::from(result.patch.is_some());
        let promotion_count = u64::from(matches!(
            decision,
            PromotionDecision::MergeSomatic | PromotionDecision::PromoteGermline { .. }
        ));

        let mut detector = detector.lock().expect("stagnation detector mutex poisoned");
        detector.record_round(test_count, successful_applies, promotion_count);

        let window = detector.capacity();
        if detector.is_stagnant(window) {
            tracing::warn!(
                window,
                suggestion = %detector.suggest_strategy_change(),
                "stagnation detected"
            );
        }
    }

    /// Update the last round with the actual apply and verify outcomes from the outer loop.
    ///
    /// Call this after attempting `git apply` and `verify_and_rebuild` in `a2ctl run`.
    /// Overwrites the provisional `successful_applies` and `promotion_count` recorded
    /// by `record_round_outcome` with ground-truth values, then re-checks for stagnation.
    pub fn record_apply_outcome(&self, applied: bool, verified: bool) {
        let Some(detector) = &self.stagnation_detector else {
            return;
        };
        let mut detector = detector.lock().expect("stagnation detector mutex poisoned");
        detector.update_last_round_apply(applied, verified);
        let window = detector.capacity();
        if detector.is_stagnant(window) {
            tracing::warn!(
                window,
                suggestion = %detector.suggest_strategy_change(),
                "stagnation detected"
            );
        }
    }
}

/// The outcome of a governor run_task cycle.
pub struct GovernorOutcome {
    pub workcell_id: WorkcellId,
    pub task_id: TaskId,
    pub result: WorkcellResult,
    pub decision: PromotionDecision,
    pub lineage: LineageRecord,
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_core::error::A2Error;
    use chrono::Utc;

    fn record_rounds(detector: &mut StagnationDetector, rounds: &[(u64, u64, u64)]) {
        for (test_count, successful_applies, promotion_count) in rounds {
            detector.record_round(*test_count, *successful_applies, *promotion_count);
        }
    }

    struct EchoCatalyst(CatalystId);

    #[async_trait::async_trait]
    impl Catalyst for EchoCatalyst {
        fn id(&self) -> &CatalystId {
            &self.0
        }
        fn name(&self) -> &str {
            "echo"
        }
        async fn execute(
            &self,
            task: &TaskContract,
            _ctx: &ContextPack,
            _model: &dyn ModelProvider,
        ) -> A2Result<PatchBundle> {
            Ok(PatchBundle {
                id: PatchId::new(),
                task_id: task.id.clone(),
                workcell_id: WorkcellId::new(),
                diff: "+hello".into(),
                rationale: "echo catalyst".into(),
                test_results: TestResults {
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                    details: vec![],
                },
                model_attribution: ModelAttribution {
                    provider: "test".into(),
                    model: "echo".into(),
                    tokens_in: 100,
                    tokens_out: 50,
                },
                created_at: Utc::now(),
            })
        }
    }

    struct PassEvaluator;

    #[async_trait::async_trait]
    impl Evaluator for PassEvaluator {
        async fn evaluate(
            &self,
            _patch: &PatchBundle,
            task: &TaskContract,
        ) -> A2Result<FitnessRecord> {
            Ok(FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task.id.clone(),
                somatic: SomaticFitness {
                    task_completed: true,
                    tests_pass: true,
                    acceptance_met: vec![true],
                    tokens_used: 150,
                    duration_secs: 0.1,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            })
        }
    }

    struct NoopModel;

    #[async_trait::async_trait]
    impl ModelProvider for NoopModel {
        async fn generate(&self, _p: &str, _s: Option<&str>) -> A2Result<GenerateResponse> {
            Ok(GenerateResponse {
                text: "noop".into(),
                tokens_in: 10,
                tokens_out: 5,
            })
        }
        fn provider_id(&self) -> &str {
            "test"
        }
        fn model_id(&self) -> &str {
            "noop"
        }
    }

    #[tokio::test]
    async fn stage0_governor_runs_task_and_promotes() {
        let gov = Governor::new(
            GermlineVersion::new(),
            Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
        );

        let task = TaskContract {
            id: TaskId::new(),
            title: "test".into(),
            description: "test task".into(),
            acceptance_criteria: vec!["it works".into()],
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
        };

        let outcome = gov
            .run_task(
                task,
                &EchoCatalyst(CatalystId::new()),
                &NoopModel,
                &PassEvaluator,
            )
            .await
            .unwrap();

        assert!(outcome.result.patch.is_some());
        assert!(matches!(
            outcome.decision,
            PromotionDecision::PromoteGermline { .. }
        ));
    }

    #[tokio::test]
    async fn stage0_governor_discards_failed_task() {
        struct FailCatalyst(CatalystId);

        #[async_trait::async_trait]
        impl Catalyst for FailCatalyst {
            fn id(&self) -> &CatalystId {
                &self.0
            }
            fn name(&self) -> &str {
                "fail"
            }
            async fn execute(
                &self,
                _t: &TaskContract,
                _c: &ContextPack,
                _m: &dyn ModelProvider,
            ) -> A2Result<PatchBundle> {
                Err(A2Error::CatalystFailure(
                    self.0.clone(),
                    "intentional".into(),
                ))
            }
        }

        let gov = Governor::new(
            GermlineVersion::new(),
            Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
        );

        let task = TaskContract {
            id: TaskId::new(),
            title: "fail".into(),
            description: "will fail".into(),
            acceptance_criteria: vec![],
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
        };

        let outcome = gov
            .run_task(
                task,
                &FailCatalyst(CatalystId::new()),
                &NoopModel,
                &PassEvaluator,
            )
            .await
            .unwrap();

        assert!(outcome.result.patch.is_none());
        assert!(matches!(
            outcome.decision,
            PromotionDecision::Discard { .. }
        ));
    }

    #[test]
    fn improving_rounds_are_not_stagnant() {
        let mut detector = StagnationDetector::new(3);
        record_rounds(&mut detector, &[(1, 0, 0), (1, 1, 0), (2, 1, 1)]);

        assert!(!detector.is_stagnant(3));
    }

    #[test]
    fn flat_rounds_trigger_stagnation_after_window_size() {
        let mut detector = StagnationDetector::new(3);
        record_rounds(&mut detector, &[(1, 0, 0), (1, 0, 0), (1, 0, 0)]);

        assert!(detector.is_stagnant(3));
    }

    #[test]
    fn a_single_improvement_resets_the_window() {
        let mut detector = StagnationDetector::new(3);
        record_rounds(&mut detector, &[(1, 0, 0), (1, 0, 0), (1, 0, 0)]);
        assert!(detector.is_stagnant(3));

        detector.record_round(2, 0, 0);
        assert!(!detector.is_stagnant(3));

        detector.record_round(2, 0, 0);
        assert!(!detector.is_stagnant(3));

        detector.record_round(2, 0, 0);
        assert!(detector.is_stagnant(3));
    }

    #[test]
    fn test_stagnation_trend() {
        let mut detector = StagnationDetector::new(5);
        assert!((detector.trend() - 0.0).abs() < f64::EPSILON);
        detector.record_round(10, 1, 1);
        assert!((detector.trend() - 0.0).abs() < f64::EPSILON);
        detector.record_round(10, 1, 3);
        assert!((detector.trend() - 2.0).abs() < f64::EPSILON);
        detector.record_round(10, 1, 2);
        assert!((detector.trend() - 0.5).abs() < f64::EPSILON);
    }
}
