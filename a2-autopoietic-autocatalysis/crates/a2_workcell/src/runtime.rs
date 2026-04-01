//! Workcell runtime — the execution environment for a single catalyst run.
//!
//! A workcell is ephemeral: it is instantiated from the germline, given a task,
//! runs a catalyst, and produces a PatchBundle. The workcell is the soma;
//! only promoted patches enter the germline.
//!
//! The runtime enforces budget limits, membrane policies, and captures
//! full lineage for every action.

use a2_core::error::A2Result;
use a2_core::id::*;
use a2_core::protocol::*;
use a2_core::traits::*;
use chrono::Utc;

use crate::budget::BudgetTracker;

/// Configuration for a workcell execution.
pub struct WorkcellConfig {
    pub workcell_id: WorkcellId,
    pub germline_version: GermlineVersion,
    pub task: TaskContract,
    pub budget: Budget,
}

/// Result of a workcell execution.
pub struct WorkcellResult {
    pub patch: Option<PatchBundle>,
    pub fitness: Option<FitnessRecord>,
    pub lineage: LineageRecord,
    pub tokens_used: u64,
    pub calls_used: u32,
    pub duration_secs: f64,
}

/// Execute a single workcell: catalyst + evaluation in a budget-bounded context.
pub async fn run_workcell(
    config: WorkcellConfig,
    catalyst: &dyn Catalyst,
    model: &dyn ModelProvider,
    evaluator: &dyn Evaluator,
) -> A2Result<WorkcellResult> {
    let tracker = BudgetTracker::new(config.budget.clone());
    let start = std::time::Instant::now();

    // Build context pack. In Stage 0, this is minimal — just the germline ref.
    let context = ContextPack {
        germline_version: config.germline_version.clone(),
        relevant_files: vec![],
        prior_attempts: vec![],
        retrieved_motifs: vec![],
    };

    // Execute the catalyst to produce a patch.
    let patch_result = catalyst.execute(&config.task, &context, model).await;

    let patch = match patch_result {
        Ok(p) => {
            // Record the model usage against budget.
            if let Err(e) = tracker.record_usage(
                p.model_attribution.tokens_in,
                p.model_attribution.tokens_out,
            ) {
                tracing::warn!(workcell = %config.workcell_id, "budget exceeded during catalyst: {e}");
                // We still have the patch — evaluate what we got.
            }
            Some(p)
        }
        Err(e) => {
            tracing::error!(workcell = %config.workcell_id, "catalyst failed: {e}");
            None
        }
    };

    // Evaluate the patch if we have one.
    let fitness = if let Some(ref p) = patch {
        match evaluator.evaluate(p, &config.task).await {
            Ok(f) => Some(f),
            Err(e) => {
                tracing::error!(workcell = %config.workcell_id, "evaluation failed: {e}");
                None
            }
        }
    } else {
        None
    };

    let duration = start.elapsed().as_secs_f64();

    // Build lineage record regardless of success/failure.
    let lineage = LineageRecord {
        id: LineageId::new(),
        task_id: config.task.id.clone(),
        patch_id: patch
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(PatchId::new),
        parent_germline: config.germline_version,
        model_attributions: patch
            .as_ref()
            .map(|p| vec![p.model_attribution.clone()])
            .unwrap_or_default(),
        fitness: fitness.clone().unwrap_or_else(|| FitnessRecord {
            eval_id: EvalId::new(),
            task_id: config.task.id.clone(),
            somatic: SomaticFitness {
                task_completed: false,
                tests_pass: false,
                acceptance_met: vec![],
                tokens_used: tracker.tokens_used(),
                duration_secs: duration,
            },
            germline: None,
            organizational: None,
            evaluated_at: Utc::now(),
        }),
        created_at: Utc::now(),
    };

    Ok(WorkcellResult {
        patch,
        fitness,
        lineage,
        tokens_used: tracker.tokens_used(),
        calls_used: tracker.calls_used(),
        duration_secs: duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_core::traits::GenerateResponse;

    struct EchoCatalyst {
        id: CatalystId,
    }

    #[async_trait::async_trait]
    impl Catalyst for EchoCatalyst {
        fn id(&self) -> &CatalystId {
            &self.id
        }
        fn name(&self) -> &str {
            "echo"
        }
        async fn execute(
            &self,
            task: &TaskContract,
            _context: &ContextPack,
            _model: &dyn ModelProvider,
        ) -> A2Result<PatchBundle> {
            Ok(PatchBundle {
                id: PatchId::new(),
                task_id: task.id.clone(),
                workcell_id: WorkcellId::new(),
                diff: "--- a/test\n+++ b/test\n+hello".into(),
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

    struct AlwaysPassEvaluator;

    #[async_trait::async_trait]
    impl Evaluator for AlwaysPassEvaluator {
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

    struct NoopProvider;

    #[async_trait::async_trait]
    impl ModelProvider for NoopProvider {
        async fn generate(
            &self,
            _prompt: &str,
            _system: Option<&str>,
        ) -> A2Result<GenerateResponse> {
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
    async fn workcell_runs_catalyst_and_evaluator() {
        let config = WorkcellConfig {
            workcell_id: WorkcellId::new(),
            germline_version: GermlineVersion::new(),
            task: TaskContract {
                id: TaskId::new(),
                title: "test task".into(),
                description: "do a thing".into(),
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
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
        };

        let catalyst = EchoCatalyst {
            id: CatalystId::new(),
        };

        let result = run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        assert!(result.patch.is_some());
        assert!(result.fitness.is_some());
        assert!(result.fitness.unwrap().somatic.task_completed);
        assert!(result.tokens_used > 0);
    }
}
