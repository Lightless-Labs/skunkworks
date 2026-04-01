//! Governor — the A² control plane.
//!
//! Orchestrates the full task lifecycle: ingest → schedule → execute → evaluate → promote.
//! Five sub-components per DESIGN.md Section 3.7: Scheduler, Selector, Promoter, Analyst, Strategist.

use a2_core::error::{A2Error, A2Result};
use a2_core::id::*;
use a2_core::protocol::*;
use a2_core::traits::*;
use a2_workcell::runtime::{WorkcellConfig, WorkcellResult, run_workcell};
use chrono::Utc;

/// Stage 0 Governor — minimal control plane for bootstrap.
///
/// In Stage 0, the governor is simple and linear:
/// 1. Accept task → 2. Create workcell → 3. Run catalyst → 4. Evaluate → 5. Decide promotion
///
/// All germline promotions require human approval (bootstrap profile B0).
pub struct Governor {
    germline_version: GermlineVersion,
    default_budget: Budget,
}

impl Governor {
    pub fn new(germline_version: GermlineVersion, default_budget: Budget) -> Self {
        Self {
            germline_version,
            default_budget,
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
}
