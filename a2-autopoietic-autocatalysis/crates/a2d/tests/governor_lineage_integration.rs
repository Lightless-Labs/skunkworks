use a2_archive::SqliteLineageStore;
use a2_core::error::A2Result;
use a2_core::id::{CatalystId, EvalId, GermlineVersion, PatchId, TaskId, WorkcellId};
use a2_core::protocol::{
    Budget, ContextPack, FitnessRecord, LineageRecord, ModelAttribution, MutationScope,
    PatchBundle, Priority, PromotionDecision, SomaticFitness, TaskContract, TaskSource,
    TestResults,
};
use a2_core::traits::{Catalyst, Evaluator, GenerateResponse, LineageStore, ModelProvider};
use a2d::Governor;
use chrono::Utc;
use rusqlite::Connection;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct MockCatalyst {
    id: CatalystId,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Catalyst for MockCatalyst {
    fn id(&self) -> &CatalystId {
        &self.id
    }

    fn name(&self) -> &str {
        "mock-catalyst"
    }

    async fn execute(
        &self,
        task: &TaskContract,
        _context: &ContextPack,
        model: &dyn ModelProvider,
    ) -> A2Result<PatchBundle> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let generated = model
            .generate("produce patch", Some("integration-test"))
            .await?;

        Ok(PatchBundle {
            id: PatchId::new(),
            task_id: task.id.clone(),
            workcell_id: WorkcellId::new(),
            diff: "--- a/test.txt\n+++ b/test.txt\n@@ -0,0 +1 @@\n+lineage".into(),
            rationale: generated.text,
            test_results: TestResults {
                passed: 1,
                failed: 0,
                skipped: 0,
                details: vec![],
            },
            model_attribution: ModelAttribution {
                provider: model.provider_id().into(),
                model: model.model_id().into(),
                tokens_in: generated.tokens_in,
                tokens_out: generated.tokens_out,
            },
            created_at: Utc::now(),
        })
    }
}

struct MockModelProvider {
    provider_id: &'static str,
    model_id: &'static str,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl ModelProvider for MockModelProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(GenerateResponse {
            text: format!(
                "generated diff for {prompt} via {}",
                system.unwrap_or("no-system")
            ),
            tokens_in: 21,
            tokens_out: 13,
        })
    }

    fn provider_id(&self) -> &str {
        self.provider_id
    }

    fn model_id(&self) -> &str {
        self.model_id
    }
}

struct RoutedMockProvider {
    provider_id: &'static str,
    model_id: &'static str,
    active: MockModelProvider,
    standby: MockModelProvider,
}

#[async_trait::async_trait]
impl ModelProvider for RoutedMockProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.active.generate(prompt, system).await
    }

    fn provider_id(&self) -> &str {
        self.provider_id
    }

    fn model_id(&self) -> &str {
        self.model_id
    }
}

struct MockEvaluator {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Evaluator for MockEvaluator {
    async fn evaluate(&self, _patch: &PatchBundle, task: &TaskContract) -> A2Result<FitnessRecord> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(FitnessRecord {
            eval_id: EvalId::new(),
            task_id: task.id.clone(),
            somatic: SomaticFitness {
                task_completed: true,
                tests_pass: true,
                acceptance_met: vec![true],
                tokens_used: 34,
                duration_secs: 0.25,
            },
            germline: None,
            organizational: None,
            evaluated_at: Utc::now(),
        })
    }
}

fn default_budget() -> Budget {
    Budget {
        max_tokens: 10_000,
        max_duration_secs: 60,
        max_calls: 10,
    }
}

fn sample_task() -> TaskContract {
    TaskContract {
        id: TaskId::new(),
        title: "record lineage".into(),
        description: "run the full governor lifecycle".into(),
        acceptance_criteria: vec!["lineage record is persisted".into()],
        budget: default_budget(),
        priority: Priority::Normal,
        source: TaskSource::External {
            origin: "integration-test".into(),
        },
        created_at: Utc::now(),
    }
}

fn assert_lineage_record_matches(actual: &LineageRecord, expected: &LineageRecord) {
    assert_eq!(actual.id, expected.id);
    assert_eq!(actual.task_id, expected.task_id);
    assert_eq!(actual.patch_id, expected.patch_id);
    assert_eq!(
        actual.parent_germline.to_string(),
        expected.parent_germline.to_string()
    );
    assert_eq!(actual.model_attributions.len(), expected.model_attributions.len());
    assert_eq!(
        actual.model_attributions[0].provider,
        expected.model_attributions[0].provider
    );
    assert_eq!(
        actual.model_attributions[0].model,
        expected.model_attributions[0].model
    );
    assert_eq!(actual.fitness.task_id, expected.fitness.task_id);
    assert_eq!(
        actual.fitness.somatic.acceptance_met,
        expected.fitness.somatic.acceptance_met
    );
}

#[tokio::test]
async fn governor_run_executes_full_task_lifecycle_with_mock_providers_and_creates_lineage_record()
{
    let governor = Governor::new(GermlineVersion::new(), default_budget());
    let task = sample_task();
    let catalyst_calls = Arc::new(AtomicUsize::new(0));
    let selected_provider_calls = Arc::new(AtomicUsize::new(0));
    let fallback_provider_calls = Arc::new(AtomicUsize::new(0));
    let evaluator_calls = Arc::new(AtomicUsize::new(0));
    let provider = RoutedMockProvider {
        provider_id: "mock-provider",
        model_id: "mock-model",
        active: MockModelProvider {
            provider_id: "mock-provider",
            model_id: "mock-model",
            calls: Arc::clone(&selected_provider_calls),
        },
        standby: MockModelProvider {
            provider_id: "unused-provider",
            model_id: "unused-model",
            calls: Arc::clone(&fallback_provider_calls),
        },
    };
    let outcome = governor
        .run_task(
            task.clone(),
            &MockCatalyst {
                id: CatalystId::new(),
                calls: Arc::clone(&catalyst_calls),
            },
            &provider,
            &MockEvaluator {
                calls: Arc::clone(&evaluator_calls),
            },
        )
        .await
        .unwrap();

    let patch = outcome.result.patch.as_ref().unwrap();
    let fitness = outcome.result.fitness.as_ref().unwrap();
    assert!(outcome.result.patch.is_some());
    assert!(outcome.result.fitness.is_some());
    assert_eq!(outcome.task_id, task.id.clone());
    assert_eq!(outcome.result.tokens_used, 34);
    assert_eq!(outcome.result.calls_used, 1);
    assert!(outcome.result.duration_secs >= 0.0);
    assert_eq!(patch.task_id, task.id.clone());
    assert_eq!(patch.model_attribution.provider, "mock-provider");
    assert_eq!(patch.model_attribution.model, "mock-model");
    assert_eq!(
        patch.rationale,
        "generated diff for produce patch via integration-test"
    );
    assert!(fitness.somatic.task_completed);
    assert!(fitness.somatic.tests_pass);
    assert_eq!(outcome.result.lineage.id, outcome.lineage.id);
    assert_eq!(outcome.result.lineage.task_id, task.id.clone());
    assert_eq!(outcome.result.lineage.patch_id, patch.id.clone());
    assert_eq!(outcome.lineage.task_id, task.id.clone());
    assert_eq!(outcome.lineage.patch_id, patch.id.clone());
    assert_eq!(outcome.lineage.model_attributions.len(), 1);
    assert_eq!(
        outcome.lineage.parent_germline.to_string(),
        outcome.result.lineage.parent_germline.to_string()
    );
    assert_eq!(outcome.lineage.fitness.task_id, task.id.clone());
    assert_eq!(outcome.lineage.fitness.somatic.acceptance_met, vec![true]);
    assert_eq!(
        outcome.lineage.model_attributions[0].provider,
        patch.model_attribution.provider
    );
    assert_eq!(
        outcome.lineage.model_attributions[0].model,
        patch.model_attribution.model
    );
    assert!(matches!(
        outcome.decision,
        PromotionDecision::PromoteGermline {
            mutation_scope: MutationScope::Prompt,
        }
    ));
    assert_eq!(catalyst_calls.load(Ordering::SeqCst), 1);
    assert_eq!(selected_provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fallback_provider_calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider.standby.provider_id(), "unused-provider");
    assert_eq!(provider.standby.model_id(), "unused-model");
    assert_eq!(evaluator_calls.load(Ordering::SeqCst), 1);

    let store = SqliteLineageStore::new(Connection::open_in_memory().unwrap()).unwrap();
    assert!(store.get(&outcome.lineage.id).await.unwrap().is_none());
    store.record(outcome.lineage.clone()).await.unwrap();

    let stored = store.get(&outcome.lineage.id).await.unwrap().unwrap();
    assert_lineage_record_matches(&stored, &outcome.lineage);
    assert_eq!(stored.task_id, task.id);

    let task_records = store.for_task(&task.id).await.unwrap();
    assert_eq!(task_records.len(), 1);
    assert_lineage_record_matches(&task_records[0], &outcome.lineage);

    let recent_records = store.recent(1).await.unwrap();
    assert_eq!(recent_records.len(), 1);
    assert_lineage_record_matches(&recent_records[0], &outcome.lineage);
}
