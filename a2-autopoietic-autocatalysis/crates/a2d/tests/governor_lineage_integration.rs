use a2_archive::SqliteLineageStore;
use a2_core::error::A2Result;
use a2_core::id::{CatalystId, EvalId, GermlineVersion, PatchId, TaskId, WorkcellId};
use a2_core::protocol::{
    Budget, ContextPack, FitnessRecord, ModelAttribution, MutationScope, PatchBundle, Priority,
    PromotionDecision, SomaticFitness, TaskContract, TaskSource, TestResults,
};
use a2_core::traits::{Catalyst, Evaluator, GenerateResponse, LineageStore, ModelProvider};
use a2d::Governor;
use chrono::Utc;
use rusqlite::Connection;

struct MockCatalyst {
    id: CatalystId,
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

struct MockModelProvider;

#[async_trait::async_trait]
impl ModelProvider for MockModelProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
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
        "mock-provider"
    }

    fn model_id(&self) -> &str {
        "mock-model"
    }
}

struct MockEvaluator;

#[async_trait::async_trait]
impl Evaluator for MockEvaluator {
    async fn evaluate(&self, _patch: &PatchBundle, task: &TaskContract) -> A2Result<FitnessRecord> {
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

#[tokio::test]
async fn governor_run_persists_lineage_record_for_completed_task() {
    let governor = Governor::new(GermlineVersion::new(), default_budget());
    let task = sample_task();
    let outcome = governor
        .run_task(
            task.clone(),
            &MockCatalyst {
                id: CatalystId::new(),
            },
            &MockModelProvider,
            &MockEvaluator,
        )
        .await
        .unwrap();

    assert!(outcome.result.patch.is_some());
    assert!(matches!(
        outcome.decision,
        PromotionDecision::PromoteGermline {
            mutation_scope: MutationScope::Prompt,
        }
    ));

    let store = SqliteLineageStore::new(Connection::open_in_memory().unwrap()).unwrap();
    store.record(outcome.lineage.clone()).await.unwrap();

    let stored = store.get(&outcome.lineage.id).await.unwrap().unwrap();
    assert_eq!(stored.id, outcome.lineage.id);
    assert_eq!(stored.task_id, task.id);
    assert_eq!(stored.patch_id, outcome.lineage.patch_id);
    assert_eq!(stored.parent_germline, outcome.lineage.parent_germline);
    assert_eq!(stored.model_attributions.len(), 1);
    assert_eq!(stored.model_attributions[0].provider, "mock-provider");
    assert_eq!(stored.model_attributions[0].model, "mock-model");
    assert!(stored.fitness.somatic.task_completed);
    assert!(stored.fitness.somatic.tests_pass);

    let task_records = store.for_task(&task.id).await.unwrap();
    assert_eq!(task_records.len(), 1);
    assert_eq!(task_records[0].id, outcome.lineage.id);
}
