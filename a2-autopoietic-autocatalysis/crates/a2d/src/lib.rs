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
use std::sync::{Arc, Mutex};

/// Actionable strategy change recommended by the stagnation detector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrategyChange {
    /// No change needed — rounds are improving.
    None,
    /// Switch to a different model provider.
    SwitchModel,
}

impl std::fmt::Display for StrategyChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "no change needed"),
            Self::SwitchModel => write!(f, "switch model provider"),
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn verifier_retry_acceptance_criteria(prior_lineage: &[LineageRecord]) -> Vec<String> {
    let mut criteria = Vec::new();
    for verification in prior_lineage
        .iter()
        .flat_map(|record| record.external_verifications.iter())
        .filter(|verification| !verification.passed)
    {
        push_unique(
            &mut criteria,
            format!(
                "Prior external verification must pass: {}",
                verification.command
            ),
        );
        for test in &verification.failing_tests {
            push_unique(
                &mut criteria,
                format!("Prior failing test must pass: {test}"),
            );
        }
        for focus in &verification.failure_focus {
            push_unique(
                &mut criteria,
                format!("Prior verifier failure must be fixed: {focus}"),
            );
        }
    }
    criteria
}

fn task_with_retry_acceptance_criteria(
    mut task: TaskContract,
    prior_lineage: &[LineageRecord],
) -> TaskContract {
    for criterion in verifier_retry_acceptance_criteria(prior_lineage) {
        push_unique(&mut task.acceptance_criteria, criterion);
    }
    task
}

fn prepend_verification_note(note: &str, existing_rationale: &str) -> String {
    let note = note.trim();
    let existing_rationale = existing_rationale.trim();

    if existing_rationale.is_empty() {
        note.to_string()
    } else if existing_rationale.starts_with("[external verify:") {
        let (_, rest) = existing_rationale
            .split_once("\n\n")
            .unwrap_or((existing_rationale, ""));
        if rest.trim().is_empty() {
            note.to_string()
        } else {
            format!("{note}\n\n{}", rest.trim())
        }
    } else {
        format!("{note}\n\n{existing_rationale}")
    }
}

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

    pub fn suggest_strategy_change(&self) -> StrategyChange {
        if self.is_stagnant(self.capacity) {
            StrategyChange::SwitchModel
        } else {
            StrategyChange::None
        }
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
    lineage_store: Option<Arc<dyn LineageStore>>,
    enable_anti_repeat_retry: bool,
}

impl Governor {
    pub fn new(germline_version: GermlineVersion, default_budget: Budget) -> Self {
        Self {
            germline_version,
            default_budget,
            stagnation_detector: None,
            lineage_store: None,
            enable_anti_repeat_retry: true,
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
            lineage_store: None,
            enable_anti_repeat_retry: true,
        }
    }

    /// Attach a lineage store for automatic persistence of lineage records.
    pub fn with_lineage_store(mut self, store: Arc<dyn LineageStore>) -> Self {
        self.lineage_store = Some(store);
        self
    }

    /// Enable or disable the anti-repeat retry prompt motif.
    ///
    /// This is normally enabled. Benchmark ablations disable only this motif
    /// while leaving prior lineage, verifier-derived relevant files, retry
    /// acceptance criteria, and candidate-worktree verifiers intact.
    pub fn with_anti_repeat_retry(mut self, enabled: bool) -> Self {
        self.enable_anti_repeat_retry = enabled;
        self
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

        // Fetch prior lineage for this task, if a store is wired. Surfaced to
        // the catalyst via ContextPack so multi-round runs can learn from
        // previous attempts. Query failure is non-fatal: fall back to empty.
        let prior_lineage = if let Some(store) = &self.lineage_store {
            match store.for_task(&task.id).await {
                Ok(records) => records,
                Err(e) => {
                    tracing::warn!(error = %e, task = %task.id, "failed to load prior lineage");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let task_for_workcell = task_with_retry_acceptance_criteria(task.clone(), &prior_lineage);

        // Stage 0 scheduler: single workcell, direct assignment.
        let config = WorkcellConfig {
            workcell_id: workcell_id.clone(),
            germline_version: self.germline_version.clone(),
            task: task_for_workcell,
            budget: self.default_budget.clone(),
            prior_lineage,
            enable_anti_repeat_retry: self.enable_anti_repeat_retry,
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

        // Persist lineage if a store is wired.
        if let Some(store) = &self.lineage_store
            && let Err(e) = store.record(lineage.clone()).await
        {
            tracing::warn!(error = %e, "failed to persist lineage record");
        }

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
        let completed_by_fitness = result
            .fitness
            .as_ref()
            .is_some_and(|fitness| fitness.somatic.task_completed);

        if completed_by_fitness || self.external_verifier_backstop_completed(result) {
            // B0: all germline mutations require human review.
            return PromotionDecision::PromoteGermline {
                mutation_scope: MutationScope::Prompt,
            };
        }

        PromotionDecision::Discard {
            reason: "task not completed or evaluation failed".into(),
        }
    }

    /// Independent task verifiers are allowed to rescue a candidate from a corrupted
    /// mutable evaluator, but only when they are explicit, all pass, tests are clean,
    /// and the outer governor budget still allows the patch.
    fn external_verifier_backstop_completed(&self, result: &WorkcellResult) -> bool {
        let Some(patch) = &result.patch else {
            return false;
        };
        if patch.worktree_verifications.is_empty() {
            return false;
        }
        if !patch
            .worktree_verifications
            .iter()
            .all(|verification| verification.passed)
        {
            return false;
        }
        if patch.test_results.failed != 0 {
            return false;
        }
        result.tokens_used <= self.default_budget.max_tokens
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

    /// Reconcile a persisted lineage record with post-apply verification truth.
    ///
    /// `run_task` records pre-apply somatic fitness from the worktree evaluator.
    /// The outer `a2ctl run --apply` path then attempts to admit that patch into
    /// the live germline and runs the full rebuild gate. Persist that later truth
    /// back into lineage so future attempts see the actual apply/rebuild outcome.
    pub async fn reconcile_lineage_apply_outcome(
        &self,
        lineage_id: &LineageId,
        applied: bool,
        verified: bool,
        verification_note: String,
        external_verification: ExternalVerification,
    ) -> A2Result<()> {
        let Some(store) = &self.lineage_store else {
            return Ok(());
        };
        let Some(mut record) = store.get(lineage_id).await? else {
            return Ok(());
        };

        let passed = applied && verified;
        record.fitness.somatic.task_completed = passed;
        record.fitness.somatic.tests_pass = passed;
        for criterion in &mut record.fitness.somatic.acceptance_met {
            *criterion = passed;
        }

        record.external_verifications.push(external_verification);
        let existing = record.patch_rationale.unwrap_or_default();
        record.patch_rationale = Some(prepend_verification_note(&verification_note, &existing));
        store.replace(record).await
    }

    /// Query the stagnation detector for a recommended strategy change.
    pub fn suggest_strategy_change(&self) -> StrategyChange {
        let Some(detector) = &self.stagnation_detector else {
            return StrategyChange::None;
        };
        let detector = detector.lock().expect("stagnation detector mutex poisoned");
        detector.suggest_strategy_change()
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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
                worktree_verifications: vec![],
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

    struct CapturingCatalyst {
        id: CatalystId,
        seen_task: Arc<Mutex<Option<TaskContract>>>,
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
            _ctx: &ContextPack,
            _model: &dyn ModelProvider,
        ) -> A2Result<PatchBundle> {
            *self.seen_task.lock().unwrap() = Some(task.clone());
            Ok(PatchBundle {
                id: PatchId::new(),
                task_id: task.id.clone(),
                workcell_id: WorkcellId::new(),
                diff: "+hello".into(),
                rationale: "captured task".into(),
                test_results: TestResults {
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                    details: vec![],
                },
                worktree_verifications: vec![],
                model_attribution: ModelAttribution {
                    provider: "test".into(),
                    model: "capture".into(),
                    tokens_in: 100,
                    tokens_out: 50,
                },
                created_at: Utc::now(),
            })
        }
    }

    struct VerifiedCatalyst(CatalystId);

    #[async_trait::async_trait]
    impl Catalyst for VerifiedCatalyst {
        fn id(&self) -> &CatalystId {
            &self.0
        }
        fn name(&self) -> &str {
            "verified"
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
                diff: "+verified".into(),
                rationale: "candidate verifier passed".into(),
                test_results: TestResults {
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                    details: vec![],
                },
                worktree_verifications: vec![ExternalVerification {
                    passed: true,
                    command: "cargo test -p a2_eval hidden".into(),
                    exit_code: Some(0),
                    failing_tests: vec![],
                    failure_focus: vec![],
                    stdout_excerpt: "ok".into(),
                    stderr_excerpt: String::new(),
                    verified_at: Utc::now(),
                }],
                model_attribution: ModelAttribution {
                    provider: "test".into(),
                    model: "verified".into(),
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

    struct CorruptEvaluator;

    #[async_trait::async_trait]
    impl Evaluator for CorruptEvaluator {
        async fn evaluate(
            &self,
            _patch: &PatchBundle,
            task: &TaskContract,
        ) -> A2Result<FitnessRecord> {
            Ok(FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task.id.clone(),
                somatic: SomaticFitness {
                    task_completed: false,
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

    #[derive(Default)]
    struct MemoryLineageStore {
        records: Mutex<HashMap<LineageId, LineageRecord>>,
    }

    #[async_trait::async_trait]
    impl LineageStore for MemoryLineageStore {
        async fn record(&self, entry: LineageRecord) -> A2Result<()> {
            self.records.lock().unwrap().insert(entry.id.clone(), entry);
            Ok(())
        }

        async fn replace(&self, entry: LineageRecord) -> A2Result<()> {
            self.records.lock().unwrap().insert(entry.id.clone(), entry);
            Ok(())
        }

        async fn get(&self, id: &LineageId) -> A2Result<Option<LineageRecord>> {
            Ok(self.records.lock().unwrap().get(id).cloned())
        }

        async fn for_task(&self, task_id: &TaskId) -> A2Result<Vec<LineageRecord>> {
            let mut records = self
                .records
                .lock()
                .unwrap()
                .values()
                .filter(|record| &record.task_id == task_id)
                .cloned()
                .collect::<Vec<_>>();
            records.sort_by_key(|record| record.created_at);
            Ok(records)
        }

        async fn recent(&self, limit: usize) -> A2Result<Vec<LineageRecord>> {
            let mut records = self
                .records
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            records.sort_by_key(|record| std::cmp::Reverse(record.created_at));
            records.truncate(limit);
            Ok(records)
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
            verification_commands: vec![],
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
    async fn candidate_verifier_backstop_promotes_when_mutable_evaluator_is_corrupt() {
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
            title: "repair evaluator".into(),
            description: "candidate verifier is authoritative".into(),
            acceptance_criteria: vec!["verifier passes".into()],
            verification_commands: vec![],
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
                &VerifiedCatalyst(CatalystId::new()),
                &NoopModel,
                &CorruptEvaluator,
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
    async fn prior_external_verification_becomes_retry_acceptance_criteria() {
        let store = Arc::new(MemoryLineageStore::default());
        let gov = Governor::new(
            GermlineVersion::new(),
            Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
        )
        .with_lineage_store(store.clone());

        let task_id = TaskId::new();
        let verification = ExternalVerification {
            passed: false,
            command: "cargo test -p a2ctl".into(),
            exit_code: Some(101),
            failing_tests: vec![
                "tests::ignores_non_task_mentions_inside_comments_and_strings".into(),
            ],
            failure_focus: vec!["assertion failed: find_scan_marker".into()],
            stdout_excerpt:
                "test tests::ignores_non_task_mentions_inside_comments_and_strings ... FAILED"
                    .into(),
            stderr_excerpt: "error: test failed".into(),
            verified_at: Utc::now(),
        };
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("+visible-only".into()),
            patch_rationale: Some("visible-only fix".into()),
            external_verifications: vec![verification.clone(), verification],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "test".into(),
                model: "noop".into(),
                tokens_in: 1,
                tokens_out: 1,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task_id.clone(),
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![false],
                    tokens_used: 2,
                    duration_secs: 0.1,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };
        store.record(prior).await.unwrap();

        let seen_task = Arc::new(Mutex::new(None));
        let task = TaskContract {
            id: task_id,
            title: "retry task".into(),
            description: "fix the visible failure".into(),
            acceptance_criteria: vec!["Original criterion remains".into()],
            verification_commands: vec![],
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

        gov.run_task(
            task,
            &CapturingCatalyst {
                id: CatalystId::new(),
                seen_task: seen_task.clone(),
            },
            &NoopModel,
            &PassEvaluator,
        )
        .await
        .unwrap();

        let captured = seen_task.lock().unwrap().clone().unwrap();
        assert!(
            captured
                .acceptance_criteria
                .contains(&"Original criterion remains".into())
        );
        assert!(
            captured
                .acceptance_criteria
                .contains(&"Prior external verification must pass: cargo test -p a2ctl".into())
        );
        assert!(captured.acceptance_criteria.contains(
            &"Prior failing test must pass: tests::ignores_non_task_mentions_inside_comments_and_strings"
                .into()
        ));
        assert!(captured.acceptance_criteria.contains(
            &"Prior verifier failure must be fixed: assertion failed: find_scan_marker".into()
        ));
        assert_eq!(
            captured
                .acceptance_criteria
                .iter()
                .filter(|criterion| criterion.as_str()
                    == "Prior external verification must pass: cargo test -p a2ctl")
                .count(),
            1,
            "duplicate prior verifier records should not duplicate retry criteria"
        );
    }

    #[tokio::test]
    async fn reconcile_lineage_apply_outcome_persists_post_apply_truth() {
        let store = Arc::new(MemoryLineageStore::default());
        let gov = Governor::new(
            GermlineVersion::new(),
            Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
        )
        .with_lineage_store(store.clone());

        let task_id = TaskId::new();
        let lineage_id = LineageId::new();
        let record = LineageRecord {
            id: lineage_id.clone(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("+candidate".into()),
            patch_rationale: Some("candidate rationale".into()),
            external_verifications: vec![],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "test".into(),
                model: "noop".into(),
                tokens_in: 1,
                tokens_out: 1,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id,
                somatic: SomaticFitness {
                    task_completed: true,
                    tests_pass: true,
                    acceptance_met: vec![true, true],
                    tokens_used: 2,
                    duration_secs: 0.1,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };
        store.record(record).await.unwrap();

        gov.reconcile_lineage_apply_outcome(
            &lineage_id,
            true,
            false,
            "[external verify: FAIL] cargo test exited 101. hidden failure".into(),
            ExternalVerification {
                passed: false,
                command: "cargo test".into(),
                exit_code: Some(101),
                failing_tests: vec!["tests::hidden".into()],
                failure_focus: vec!["hidden failure".into()],
                stdout_excerpt: "hidden failure".into(),
                stderr_excerpt: "error: test failed".into(),
                verified_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let reconciled = store.get(&lineage_id).await.unwrap().unwrap();
        assert!(!reconciled.fitness.somatic.task_completed);
        assert!(!reconciled.fitness.somatic.tests_pass);
        assert_eq!(
            reconciled.fitness.somatic.acceptance_met,
            vec![false, false]
        );
        assert_eq!(
            reconciled.patch_rationale.as_deref(),
            Some(
                "[external verify: FAIL] cargo test exited 101. hidden failure\n\ncandidate rationale"
            )
        );
        assert_eq!(reconciled.external_verifications.len(), 1);
        assert_eq!(reconciled.external_verifications[0].command, "cargo test");
        assert_eq!(reconciled.external_verifications[0].exit_code, Some(101));
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
            verification_commands: vec![],
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

    #[test]
    fn strategy_change_matches_actions_a2ctl_actually_supports() {
        let mut detector = StagnationDetector::new(3);
        record_rounds(&mut detector, &[(1, 0, 0), (1, 1, 0), (2, 1, 1)]);
        assert_eq!(detector.suggest_strategy_change(), StrategyChange::None);

        let mut no_promotions = StagnationDetector::new(3);
        record_rounds(&mut no_promotions, &[(1, 0, 0), (1, 0, 0), (1, 0, 0)]);
        assert_eq!(
            no_promotions.suggest_strategy_change(),
            StrategyChange::SwitchModel
        );

        let mut flat_promotions = StagnationDetector::new(3);
        record_rounds(&mut flat_promotions, &[(1, 1, 1), (1, 1, 1), (1, 1, 1)]);
        assert_eq!(
            flat_promotions.suggest_strategy_change(),
            StrategyChange::SwitchModel
        );
    }
}
