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
    /// Prior lineage records for the same task, oldest first.
    /// Populated by the Governor from the LineageStore; empty on first attempt.
    pub prior_lineage: Vec<LineageRecord>,
}

/// Normalize and bound persisted patch text before putting it into a prompt motif.
fn compact_snippet(value: &str, max_chars: usize) -> String {
    let normalized = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let mut truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

/// Render a prior LineageRecord as a compact motif line for the context pack.
fn render_prior_motif(record: &LineageRecord, index: usize) -> String {
    let model = record
        .model_attributions
        .first()
        .map(|a| format!("{}/{}", a.provider, a.model))
        .unwrap_or_else(|| "unknown".into());
    let s = &record.fitness.somatic;
    let mut motif = format!(
        "attempt {} [{}]: task_completed={}, tests_pass={}, tokens={}, duration={:.1}s",
        index + 1,
        model,
        s.task_completed,
        s.tests_pass,
        s.tokens_used,
        s.duration_secs
    );

    if let Some(rationale) = record
        .patch_rationale
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        motif.push_str(&format!(
            ", rationale=\"{}\"",
            compact_snippet(rationale, 220)
        ));
    }

    if let Some(diff) = record
        .patch_diff
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        motif.push_str(&format!(", diff=\"{}\"", compact_snippet(diff, 320)));
    }

    motif
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

    // Build context pack. Prior lineage (if any) is surfaced to the catalyst
    // via prior_attempts (IDs) + retrieved_motifs (compact rendered summaries).
    let prior_attempts = config.prior_lineage.iter().map(|r| r.id.clone()).collect();
    let retrieved_motifs = config
        .prior_lineage
        .iter()
        .enumerate()
        .map(|(i, r)| render_prior_motif(r, i))
        .collect();
    let context = ContextPack {
        germline_version: config.germline_version.clone(),
        relevant_files: vec![],
        prior_attempts,
        retrieved_motifs,
    };

    // Execute the catalyst to produce a patch, bounded by the wall-clock budget.
    let timeout_duration = std::time::Duration::from_secs(config.budget.max_duration_secs);
    let timed = tokio::time::timeout(
        timeout_duration,
        catalyst.execute(&config.task, &context, model),
    )
    .await;

    let patch = match timed {
        Err(_elapsed) => {
            tracing::warn!(
                workcell = %config.workcell_id,
                timeout_secs = config.budget.max_duration_secs,
                "catalyst timed out — wall-clock budget exceeded"
            );
            None
        }
        Ok(Ok(p)) => {
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
        Ok(Err(e)) => {
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
        patch_diff: patch.as_ref().map(|p| p.diff.clone()),
        patch_rationale: patch.as_ref().map(|p| p.rationale.clone()),
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

    /// Captures the ContextPack the catalyst was invoked with so the test can
    /// assert that prior_lineage is surfaced correctly.
    struct CapturingCatalyst {
        id: CatalystId,
        seen: std::sync::Arc<std::sync::Mutex<Option<ContextPack>>>,
    }

    #[async_trait::async_trait]
    impl Catalyst for CapturingCatalyst {
        fn id(&self) -> &CatalystId {
            &self.id
        }
        fn name(&self) -> &str {
            "capturing"
        }
        async fn execute(
            &self,
            task: &TaskContract,
            context: &ContextPack,
            _model: &dyn ModelProvider,
        ) -> A2Result<PatchBundle> {
            *self.seen.lock().unwrap() = Some(context.clone());
            Ok(PatchBundle {
                id: PatchId::new(),
                task_id: task.id.clone(),
                workcell_id: WorkcellId::new(),
                diff: "+x".into(),
                rationale: "capture".into(),
                test_results: TestResults {
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    details: vec![],
                },
                model_attribution: ModelAttribution {
                    provider: "t".into(),
                    model: "m".into(),
                    tokens_in: 1,
                    tokens_out: 1,
                },
                created_at: Utc::now(),
            })
        }
    }

    #[tokio::test]
    async fn prior_lineage_surfaces_as_attempts_and_motifs() {
        let task_id = TaskId::new();
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("--- a/foo\n+++ b/foo\n+bad approach".into()),
            patch_rationale: Some("tried the wrong file".into()),
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "gemini".into(),
                model: "gemini-3.1-pro-preview".into(),
                tokens_in: 2000,
                tokens_out: 500,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task_id.clone(),
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![],
                    tokens_used: 2500,
                    duration_secs: 42.0,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };
        let prior_id = prior.id.clone();

        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let catalyst = CapturingCatalyst {
            id: CatalystId::new(),
            seen: seen.clone(),
        };

        let config = WorkcellConfig {
            workcell_id: WorkcellId::new(),
            germline_version: GermlineVersion::new(),
            task: TaskContract {
                id: task_id,
                title: "t".into(),
                description: "d".into(),
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
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![prior],
        };

        run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        let captured = seen
            .lock()
            .unwrap()
            .clone()
            .expect("catalyst must see context");
        assert_eq!(captured.prior_attempts, vec![prior_id]);
        assert_eq!(captured.retrieved_motifs.len(), 1);
        let motif = &captured.retrieved_motifs[0];
        assert!(motif.contains("attempt 1"));
        assert!(motif.contains("gemini/gemini-3.1-pro-preview"));
        assert!(motif.contains("task_completed=false"));
        assert!(motif.contains("tests_pass=false"));
        assert!(motif.contains("rationale=\"tried the wrong file\""));
        assert!(motif.contains("diff=\"--- a/foo +++ b/foo +bad approach\""));
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
            prior_lineage: vec![],
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
        assert_eq!(
            result.lineage.patch_diff.as_deref(),
            Some("--- a/test\n+++ b/test\n+hello")
        );
        assert_eq!(
            result.lineage.patch_rationale.as_deref(),
            Some("echo catalyst")
        );
        assert!(result.tokens_used > 0);
    }
}
