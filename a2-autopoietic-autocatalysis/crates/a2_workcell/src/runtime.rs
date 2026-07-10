//! Workcell runtime — the execution environment for a single catalyst run.
//!
//! A workcell is ephemeral: it is instantiated from the germline, given a task,
//! runs a catalyst, and produces a PatchBundle. The workcell is the soma;
//! only promoted patches enter the germline.
//!
//! The runtime enforces budget limits, membrane policies, and captures
//! full lineage for every action.

use a2_core::error::{A2Error, A2Result};
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
    /// Emit the anti-repeat retry prompt motif when prior failed patch shape
    /// misses verifier-derived source paths. Keep configurable for benchmark
    /// ablations that isolate this strategy from the candidate verifier.
    pub enable_anti_repeat_retry: bool,
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

fn verification_failure_focus(value: &str, max_chars: usize) -> Option<String> {
    let mut focused = Vec::new();
    for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let lower = line.to_ascii_lowercase();
        if lower.contains("failed")
            || lower.contains("failures:")
            || lower.contains("panicked at")
            || lower.contains("assertion failed")
            || lower.contains("assertion `")
            || lower.contains("left:")
            || lower.contains("right:")
        {
            focused.push(line);
        }
    }

    if focused.is_empty() {
        None
    } else {
        Some(compact_snippet(&focused.join("\n"), max_chars))
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn push_unique_path(paths: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn extract_rust_source_paths(value: &str) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for token in value.split_whitespace() {
        let token = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        });
        let Some(end) = token.find(".rs") else {
            continue;
        };
        let candidate = &token[..end + 3];
        if candidate.contains('/') {
            push_unique_path(&mut paths, std::path::PathBuf::from(candidate));
        }
    }
    paths
}

fn failed_verification_source_paths(
    verification: &ExternalVerification,
) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for value in verification
        .failure_focus
        .iter()
        .chain(std::iter::once(&verification.stdout_excerpt))
        .chain(std::iter::once(&verification.stderr_excerpt))
    {
        for path in extract_rust_source_paths(value) {
            push_unique_path(&mut files, path);
        }
    }
    files
}

fn verifier_derived_relevant_files(prior_lineage: &[LineageRecord]) -> Vec<std::path::PathBuf> {
    const MAX_RELEVANT_FILES: usize = 8;
    let mut files = Vec::new();
    for verification in prior_lineage
        .iter()
        .flat_map(|record| record.external_verifications.iter())
        .filter(|verification| !verification.passed)
    {
        for path in failed_verification_source_paths(verification) {
            push_unique_path(&mut files, path);
            if files.len() >= MAX_RELEVANT_FILES {
                return files;
            }
        }
    }
    files
}

fn normalize_diff_path(raw: &str) -> Option<std::path::PathBuf> {
    let path = raw.trim();
    if path.is_empty() || path == "/dev/null" {
        return None;
    }
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);
    Some(std::path::PathBuf::from(path))
}

fn touched_files_from_diff(diff: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for line in diff.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let mut parts = rest.split_whitespace();
            let _old = parts.next();
            if let Some(new) = parts.next().and_then(normalize_diff_path) {
                push_unique_path(&mut files, new);
            }
        } else if let Some(path) = line.strip_prefix("+++ ").and_then(normalize_diff_path) {
            push_unique_path(&mut files, path);
        }
    }
    files
}

fn path_list(paths: &[std::path::PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sorted_path_key(paths: &[std::path::PathBuf]) -> Vec<String> {
    let mut key = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    key.sort();
    key
}

fn render_anti_repeat_retry_motif(prior_lineage: &[LineageRecord]) -> Option<String> {
    let latest_failed = prior_lineage
        .iter()
        .rev()
        .find(|record| record.external_verifications.iter().any(|v| !v.passed))?;
    let touched_files = latest_failed
        .patch_diff
        .as_deref()
        .map(touched_files_from_diff)
        .unwrap_or_default();
    if touched_files.is_empty() {
        return None;
    }

    let mut unresolved_files = Vec::new();
    let mut failure_focus = Vec::new();
    for verification in latest_failed
        .external_verifications
        .iter()
        .filter(|verification| !verification.passed)
    {
        for path in failed_verification_source_paths(verification) {
            push_unique_path(&mut unresolved_files, path);
        }
        if let Some(focus) = external_verification_focus(verification) {
            push_unique(&mut failure_focus, focus);
        }
    }
    if unresolved_files.is_empty() {
        return None;
    }

    if unresolved_files
        .iter()
        .any(|path| touched_files.iter().any(|touched| touched == path))
    {
        return None;
    }

    let latest_key = sorted_path_key(&touched_files);
    let repeated_count = prior_lineage
        .iter()
        .filter(|record| record.external_verifications.iter().any(|v| !v.passed))
        .filter_map(|record| record.patch_diff.as_deref().map(touched_files_from_diff))
        .filter(|files| !files.is_empty() && sorted_path_key(files) == latest_key)
        .count();

    let mut motif = format!(
        "anti_repeat_retry:\n  prior_touched_files: {}\n  unresolved_verifier_files: {}\n  warning: Prior failed patch shape did not touch the files named by unresolved verifier failures. Do not repeat the previous patch shape alone; inspect and address the unresolved verifier failure.",
        path_list(&touched_files),
        path_list(&unresolved_files)
    );
    if repeated_count > 1 {
        motif.push_str(&format!(
            "\n  repeated_failed_patch_shape: same touched-file set has failed {repeated_count} times"
        ));
    }
    if !failure_focus.is_empty() {
        motif.push_str(&format!(
            "\n  failure_focus: {}",
            compact_snippet(&failure_focus.join("\n"), 1_200)
        ));
    }
    Some(motif)
}

/// Split a benchmark/run verification note out of persisted model rationale.
///
/// Self-correction runs currently prepend notes like
/// `[external verify: FAIL] cargo test ... exited 101.` to `patch_rationale` so
/// the next attempt can see the real post-apply verification outcome. Keep that
/// signal separate from the model's own rationale so it cannot be buried behind
/// rationale truncation.
fn split_external_verify_note(value: &str) -> (Option<String>, String) {
    let trimmed = value.trim();
    let Some(start) = trimmed.find("[external verify:") else {
        return (None, trimmed.to_string());
    };

    let before = trimmed[..start].trim();
    let after_marker = &trimmed[start..];
    let (note, after) = after_marker
        .split_once("\n\n")
        .unwrap_or((after_marker, ""));

    let mut remainder = String::new();
    if !before.is_empty() {
        remainder.push_str(before);
    }
    let after = after.trim();
    if !after.is_empty() {
        if !remainder.is_empty() {
            remainder.push_str("\n\n");
        }
        remainder.push_str(after);
    }

    (Some(note.trim().to_string()), remainder)
}

fn external_verification_detail(verification: &ExternalVerification) -> String {
    let mut parts = vec![format!("command={}", verification.command)];
    if let Some(exit_code) = verification.exit_code {
        parts.push(format!("exit_code={exit_code}"));
    }
    if !verification.failing_tests.is_empty() {
        parts.push(format!(
            "failing_tests={}",
            verification.failing_tests.join(", ")
        ));
    }
    if !verification.stdout_excerpt.trim().is_empty() {
        parts.push(format!("stdout={}", verification.stdout_excerpt.trim()));
    }
    if !verification.stderr_excerpt.trim().is_empty() {
        parts.push(format!("stderr={}", verification.stderr_excerpt.trim()));
    }
    compact_snippet(&parts.join("; "), 1_200)
}

fn external_verification_focus(verification: &ExternalVerification) -> Option<String> {
    if !verification.failure_focus.is_empty() {
        return Some(compact_snippet(
            &verification.failure_focus.join("\n"),
            1_200,
        ));
    }
    if !verification.failing_tests.is_empty() {
        return Some(compact_snippet(
            &verification.failing_tests.join("\n"),
            1_200,
        ));
    }

    let combined = format!(
        "{}\n{}",
        verification.stdout_excerpt, verification.stderr_excerpt
    );
    verification_failure_focus(&combined, 1_200)
}

fn benchmark_sensitive_runtime_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("senior swe bench metadata")
        || lower.contains("swe-bench pro metadata")
        || lower.contains("problem_statement")
        || lower.contains("# task:")
        || lower.contains("## task")
}

fn task_requires_redacted_catalyst_errors(task: &TaskContract) -> bool {
    let source_is_benchmark = match &task.source {
        TaskSource::External { origin } => {
            let origin = origin.to_ascii_lowercase();
            origin.contains("senior-swe-bench")
                || origin.contains("swe-bench")
                || origin.contains("swebench")
        }
        TaskSource::Internal { .. } => false,
        TaskSource::Sensorium { evidence_id } => benchmark_sensitive_runtime_text(evidence_id),
    };
    task.no_external_solution_search
        || source_is_benchmark
        || benchmark_sensitive_runtime_text(&task.title)
        || benchmark_sensitive_runtime_text(&task.description)
}

fn safe_catalyst_error_focus(error: &A2Error, task: &TaskContract) -> String {
    let raw = match error {
        A2Error::CatalystFailure(_, message)
        | A2Error::ProviderError(message)
        | A2Error::PromotionRejected(message)
        | A2Error::RollbackRequired(message)
        | A2Error::MembraneDenied(message) => message.as_str(),
        A2Error::TaskRejected(_, message) => message.as_str(),
        A2Error::BudgetExceeded(_, message) => message.as_str(),
        A2Error::Timeout { operation, .. } => operation.as_str(),
        A2Error::InvariantViolation { detail, .. } => detail.as_str(),
        A2Error::ConstitutionalViolation { clause } => clause.as_str(),
        A2Error::Io(_) | A2Error::Json(_) => {
            "catalyst execution failed before producing a candidate patch"
        }
    };
    if task_requires_redacted_catalyst_errors(task) || benchmark_sensitive_runtime_text(raw) {
        "catalyst execution failed before producing a candidate patch; detailed error withheld to avoid leaking benchmark task text".into()
    } else {
        compact_snippet(raw, 600)
    }
}

fn catalyst_failure_verification(failure_focus: String) -> ExternalVerification {
    ExternalVerification {
        passed: false,
        command: "catalyst.execute".into(),
        exit_code: None,
        failing_tests: vec![],
        failure_focus: vec![failure_focus],
        stdout_excerpt: String::new(),
        stderr_excerpt: String::new(),
        verified_at: Utc::now(),
    }
}

/// Render a prior LineageRecord as a prompt motif for the context pack.
fn render_prior_motif(record: &LineageRecord, index: usize) -> String {
    let model = record
        .model_attributions
        .first()
        .map(|a| format!("{}/{}", a.provider, a.model))
        .unwrap_or_else(|| "unknown".into());
    let s = &record.fitness.somatic;

    let rationale = record
        .patch_rationale
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let (external_verify, rationale_without_verify) = rationale
        .map(split_external_verify_note)
        .unwrap_or_else(|| (None, String::new()));

    if let Some(verification) = record
        .external_verifications
        .iter()
        .rev()
        .find(|verification| !verification.passed)
    {
        let mut motif = format!(
            "attempt {} [{}]\n  status: task_completed={}, tests_pass={}, tokens={}, duration={:.1}s\n  external_verification:\n    result: FAIL\n    command: {}",
            index + 1,
            model,
            s.task_completed,
            s.tests_pass,
            s.tokens_used,
            s.duration_secs,
            verification.command,
        );
        if let Some(focus) = external_verification_focus(verification) {
            motif.push_str(&format!("\n    failure_focus: {focus}"));
        }
        motif.push_str(&format!(
            "\n    detail: {}",
            external_verification_detail(verification)
        ));

        if !rationale_without_verify.trim().is_empty() {
            motif.push_str(&format!(
                "\n  rationale: \"{}\"",
                compact_snippet(&rationale_without_verify, 220)
            ));
        }

        if let Some(diff) = record
            .patch_diff
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            motif.push_str(&format!("\n  diff: \"{}\"", compact_snippet(diff, 320)));
        }

        return motif;
    }

    if let Some(note) = external_verify
        .as_deref()
        .filter(|note| note.contains("[external verify: FAIL]"))
    {
        let mut motif = format!(
            "attempt {} [{}]\n  status: task_completed={}, tests_pass={}, tokens={}, duration={:.1}s\n  external_verification:\n    result: FAIL",
            index + 1,
            model,
            s.task_completed,
            s.tests_pass,
            s.tokens_used,
            s.duration_secs,
        );
        if let Some(focus) = verification_failure_focus(note, 1_200) {
            motif.push_str(&format!("\n    failure_focus: {focus}"));
        }
        motif.push_str(&format!("\n    detail: {}", compact_snippet(note, 1_200)));

        if !rationale_without_verify.trim().is_empty() {
            motif.push_str(&format!(
                "\n  rationale: \"{}\"",
                compact_snippet(&rationale_without_verify, 220)
            ));
        }

        if let Some(diff) = record
            .patch_diff
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            motif.push_str(&format!("\n  diff: \"{}\"", compact_snippet(diff, 320)));
        }

        return motif;
    }

    let mut motif = format!(
        "attempt {} [{}]: task_completed={}, tests_pass={}, tokens={}, duration={:.1}s",
        index + 1,
        model,
        s.task_completed,
        s.tests_pass,
        s.tokens_used,
        s.duration_secs
    );

    if let Some(verification) = record.external_verifications.iter().next_back() {
        let result = if verification.passed { "PASS" } else { "FAIL" };
        motif.push_str(&format!(
            ", external_verify=\"result={result}; {}\"",
            external_verification_detail(verification)
        ));
    } else if let Some(note) = external_verify.as_deref() {
        motif.push_str(&format!(
            ", external_verify=\"{}\"",
            compact_snippet(note, 220)
        ));
    }

    if !rationale_without_verify.trim().is_empty() {
        motif.push_str(&format!(
            ", rationale=\"{}\"",
            compact_snippet(&rationale_without_verify, 220)
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
    let relevant_files = verifier_derived_relevant_files(&config.prior_lineage);
    let mut retrieved_motifs = config
        .prior_lineage
        .iter()
        .enumerate()
        .map(|(i, r)| render_prior_motif(r, i))
        .collect::<Vec<_>>();
    if config.enable_anti_repeat_retry
        && let Some(motif) = render_anti_repeat_retry_motif(&config.prior_lineage)
    {
        retrieved_motifs.push(motif);
    }
    let context = ContextPack {
        germline_version: config.germline_version.clone(),
        relevant_files,
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

    let mut no_candidate_verifications = Vec::new();
    let patch = match timed {
        Err(_elapsed) => {
            tracing::warn!(
                workcell = %config.workcell_id,
                timeout_secs = config.budget.max_duration_secs,
                "catalyst timed out — wall-clock budget exceeded"
            );
            no_candidate_verifications.push(catalyst_failure_verification(format!(
                "catalyst timed out before producing a candidate patch after {}s wall-clock budget",
                config.budget.max_duration_secs
            )));
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
            no_candidate_verifications.push(catalyst_failure_verification(
                safe_catalyst_error_focus(&e, &config.task),
            ));
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
        external_verifications: patch
            .as_ref()
            .map(|p| p.worktree_verifications.clone())
            .unwrap_or(no_candidate_verifications),
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
                worktree_verifications: vec![],
                network_policy_enforced: None,
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

    fn failed_lineage_with_diff(
        task_id: &TaskId,
        diff: &str,
        failure_focus: Vec<&str>,
    ) -> LineageRecord {
        LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some(diff.into()),
            patch_rationale: Some("visible-only fix".into()),
            external_verifications: vec![ExternalVerification {
                passed: false,
                command: "cargo test -p a2ctl".into(),
                exit_code: Some(101),
                failing_tests: vec![
                    "tests::ignores_non_task_mentions_inside_comments_and_strings".into(),
                ],
                failure_focus: failure_focus.into_iter().map(String::from).collect(),
                stdout_excerpt: String::new(),
                stderr_excerpt: "error: test failed".into(),
                verified_at: Utc::now(),
            }],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "opencode".into(),
                model: "minimax".into(),
                tokens_in: 1000,
                tokens_out: 234,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task_id.clone(),
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![false],
                    tokens_used: 1234,
                    duration_secs: 7.5,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        }
    }

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

    struct VerifierCatalyst {
        id: CatalystId,
        verification: ExternalVerification,
    }

    struct FailingCatalyst {
        id: CatalystId,
        message: String,
    }

    #[async_trait::async_trait]
    impl Catalyst for VerifierCatalyst {
        fn id(&self) -> &CatalystId {
            &self.id
        }
        fn name(&self) -> &str {
            "verifier"
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
                diff: "+verified".into(),
                rationale: "candidate with verifier output".into(),
                test_results: TestResults {
                    passed: 0,
                    failed: 1,
                    skipped: 0,
                    details: vec![TestDetail {
                        name: self.verification.command.clone(),
                        passed: false,
                        output: Some("hidden failure".into()),
                    }],
                },
                worktree_verifications: vec![self.verification.clone()],
                network_policy_enforced: None,
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

    #[async_trait::async_trait]
    impl Catalyst for FailingCatalyst {
        fn id(&self) -> &CatalystId {
            &self.id
        }
        fn name(&self) -> &str {
            "failing"
        }
        async fn execute(
            &self,
            _task: &TaskContract,
            _context: &ContextPack,
            _model: &dyn ModelProvider,
        ) -> A2Result<PatchBundle> {
            Err(A2Error::CatalystFailure(
                self.id.clone(),
                self.message.clone(),
            ))
        }
    }

    fn test_task(task_id: TaskId) -> TaskContract {
        TaskContract {
            id: task_id,
            title: "test task".into(),
            description: "do a thing".into(),
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
            no_external_solution_search: false,
            network_policy: None,
            created_at: Utc::now(),
        }
    }

    fn test_workcell_config(task: TaskContract) -> WorkcellConfig {
        WorkcellConfig {
            workcell_id: WorkcellId::new(),
            germline_version: GermlineVersion::new(),
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            task,
            prior_lineage: vec![],
            enable_anti_repeat_retry: true,
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
                worktree_verifications: vec![],
                network_policy_enforced: None,
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
            external_verifications: vec![],
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
                no_external_solution_search: false,
                network_policy: None,
                created_at: Utc::now(),
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![prior],
            enable_anti_repeat_retry: true,
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
    async fn prior_external_verification_paths_become_relevant_files() {
        let task_id = TaskId::new();
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("+visible-only".into()),
            patch_rationale: Some("visible-only fix".into()),
            external_verifications: vec![ExternalVerification {
                passed: false,
                command: "cargo test -p a2ctl".into(),
                exit_code: Some(101),
                failing_tests: vec![
                    "tests::ignores_non_task_mentions_inside_comments_and_strings".into(),
                ],
                failure_focus: vec![
                    "thread 'tests::ignores_non_task_mentions_inside_comments_and_strings' panicked at crates/a2ctl/src/main.rs:1556:9:".into(),
                    "assertion failed: find_scan_marker".into(),
                ],
                stdout_excerpt: "thread panicked at crates/a2ctl/src/main.rs:1556:9".into(),
                stderr_excerpt: "error: test failed".into(),
                verified_at: Utc::now(),
            }],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "opencode".into(),
                model: "minimax".into(),
                tokens_in: 1000,
                tokens_out: 234,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id: task_id.clone(),
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![false],
                    tokens_used: 1234,
                    duration_secs: 7.5,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };
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
                no_external_solution_search: false,
                network_policy: None,
                created_at: Utc::now(),
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![prior],
            enable_anti_repeat_retry: true,
        };

        run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        let captured = seen.lock().unwrap().clone().unwrap();
        assert_eq!(
            captured.relevant_files,
            vec![std::path::PathBuf::from("crates/a2ctl/src/main.rs")]
        );
    }

    #[test]
    fn anti_repeat_retry_motif_warns_when_failed_patch_shape_misses_verifier_file() {
        let task_id = TaskId::new();
        let visible_only_diff = "diff --git a/crates/a2_core/src/lib.rs b/crates/a2_core/src/lib.rs\n\
            --- a/crates/a2_core/src/lib.rs\n\
            +++ b/crates/a2_core/src/lib.rs\n\
            @@\n\
            +visible-only fix";
        let focus = vec![
            "thread 'tests::ignores_non_task_mentions_inside_comments_and_strings' panicked at crates/a2ctl/src/main.rs:1556:9:",
            "assertion failed: find_scan_marker",
        ];
        let first = failed_lineage_with_diff(&task_id, visible_only_diff, focus.clone());
        let second = failed_lineage_with_diff(&task_id, visible_only_diff, focus);

        let motif = render_anti_repeat_retry_motif(&[first, second])
            .expect("visible-only repeated failure should produce an anti-repeat warning");

        assert!(motif.contains("anti_repeat_retry:"));
        assert!(motif.contains("prior_touched_files: `crates/a2_core/src/lib.rs`"));
        assert!(motif.contains("unresolved_verifier_files: `crates/a2ctl/src/main.rs`"));
        assert!(motif.contains(
            "Prior failed patch shape did not touch the files named by unresolved verifier failures"
        ));
        assert!(motif.contains("Do not repeat the previous patch shape alone"));
        assert!(motif.contains("same touched-file set has failed 2 times"));
        assert!(motif.contains("assertion failed: find_scan_marker"));
    }

    #[test]
    fn anti_repeat_retry_motif_is_absent_when_patch_touches_verifier_file() {
        let task_id = TaskId::new();
        let hidden_file_diff = "diff --git a/crates/a2ctl/src/main.rs b/crates/a2ctl/src/main.rs\n\
            --- a/crates/a2ctl/src/main.rs\n\
            +++ b/crates/a2ctl/src/main.rs\n\
            @@\n\
            +hidden verifier fix";
        let prior = failed_lineage_with_diff(
            &task_id,
            hidden_file_diff,
            vec![
                "thread 'tests::ignores_non_task_mentions_inside_comments_and_strings' panicked at crates/a2ctl/src/main.rs:1556:9:",
            ],
        );

        assert!(render_anti_repeat_retry_motif(&[prior]).is_none());
    }

    #[tokio::test]
    async fn anti_repeat_retry_motif_can_be_disabled_for_ablation() {
        let task_id = TaskId::new();
        let visible_only_diff = "diff --git a/crates/a2_core/src/lib.rs b/crates/a2_core/src/lib.rs\n\
            --- a/crates/a2_core/src/lib.rs\n\
            +++ b/crates/a2_core/src/lib.rs\n\
            @@\n\
            +visible-only fix";
        let prior = failed_lineage_with_diff(
            &task_id,
            visible_only_diff,
            vec!["thread panicked at crates/a2ctl/src/main.rs:1556:9"],
        );
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
                title: "ablation".into(),
                description: "disable anti-repeat only".into(),
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
                no_external_solution_search: false,
                network_policy: None,
                created_at: Utc::now(),
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![prior],
            enable_anti_repeat_retry: false,
        };

        run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        let captured = seen.lock().unwrap().clone().unwrap();
        assert_eq!(captured.retrieved_motifs.len(), 1);
        assert!(
            !captured
                .retrieved_motifs
                .iter()
                .any(|motif| motif.contains("anti_repeat_retry:"))
        );
    }

    #[test]
    fn external_verification_failures_render_as_prominent_multiline_motifs() {
        let task_id = TaskId::new();
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("--- a/crate/src/lib.rs\n+++ b/crate/src/lib.rs\n+visible-only fix".into()),
            patch_rationale: Some(
                "[external verify: FAIL] cargo test -p hidden-fixture exited 101. assertion failed: hidden edge case still broken\n\ntried visible fix only"
                    .into(),
            ),
            external_verifications: vec![],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "opencode".into(),
                model: "minimax-coding-plan/MiniMax-M2.7".into(),
                tokens_in: 1000,
                tokens_out: 234,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id,
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![false],
                    tokens_used: 1234,
                    duration_secs: 7.5,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };

        let motif = render_prior_motif(&prior, 0);

        assert!(motif.contains("attempt 1 [opencode/minimax-coding-plan/MiniMax-M2.7]"));
        assert!(motif.contains(
            "\n  status: task_completed=false, tests_pass=false, tokens=1234, duration=7.5s"
        ));
        assert!(motif.contains("\n  external_verification:"));
        assert!(motif.contains("\n    result: FAIL"));
        assert!(motif.contains(
            "failure_focus: [external verify: FAIL] cargo test -p hidden-fixture exited 101. assertion failed: hidden edge case still broken"
        ));
        assert!(motif.contains(
            "detail: [external verify: FAIL] cargo test -p hidden-fixture exited 101. assertion failed: hidden edge case still broken"
        ));
        assert!(motif.contains("\n  rationale: \"tried visible fix only\""));
        assert!(motif.contains(
            "\n  diff: \"--- a/crate/src/lib.rs +++ b/crate/src/lib.rs +visible-only fix\""
        ));
        assert!(!motif.contains("rationale=\"[external verify: FAIL]"));
    }

    #[test]
    fn structured_external_verification_renders_before_legacy_rationale_markers() {
        let task_id = TaskId::new();
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: Some("--- a/crate/src/lib.rs\n+++ b/crate/src/lib.rs\n+visible-only fix".into()),
            patch_rationale: Some(
                "[external verify: FAIL] stale prose should not drive the motif\n\ntried visible fix only"
                    .into(),
            ),
            external_verifications: vec![ExternalVerification {
                passed: false,
                command: "cargo test -p a2ctl".into(),
                exit_code: Some(101),
                failing_tests: vec![
                    "tests::ignores_non_task_mentions_inside_comments_and_strings".into(),
                ],
                failure_focus: vec![
                    "test tests::ignores_non_task_mentions_inside_comments_and_strings ... FAILED"
                        .into(),
                    "assertion failed: find_scan_marker".into(),
                ],
                stdout_excerpt:
                    "test tests::ignores_non_task_mentions_inside_comments_and_strings ... FAILED"
                        .into(),
                stderr_excerpt: "error: test failed".into(),
                verified_at: Utc::now(),
            }],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![ModelAttribution {
                provider: "opencode".into(),
                model: "minimax-coding-plan/MiniMax-M2.7".into(),
                tokens_in: 1000,
                tokens_out: 234,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id,
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![false],
                    tokens_used: 1234,
                    duration_secs: 7.5,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };

        let motif = render_prior_motif(&prior, 0);

        assert!(motif.contains("\n  external_verification:"));
        assert!(motif.contains("\n    command: cargo test -p a2ctl"));
        assert!(motif.contains("failure_focus: test tests::ignores_non_task_mentions_inside_comments_and_strings ... FAILED assertion failed: find_scan_marker"));
        assert!(motif.contains("detail: command=cargo test -p a2ctl; exit_code=101"));
        assert!(motif.contains(
            "failing_tests=tests::ignores_non_task_mentions_inside_comments_and_strings"
        ));
        assert!(motif.contains("\n  rationale: \"tried visible fix only\""));
        assert!(!motif.contains("stale prose should not drive the motif"));
    }

    #[test]
    fn external_verification_focus_preserves_late_hidden_failures() {
        let note = "[external verify: FAIL] verify_and_rebuild failed. cargo test failed: stdout:\n\
            running 99 tests\n\
            test many::passing::tests ... ok\n\
            test tests::ignores_non_task_mentions_inside_comments_and_strings ... FAILED\n\
            failures:\n\n\
            ---- tests::ignores_non_task_mentions_inside_comments_and_strings stdout ----\n\
            thread 'tests::ignores_non_task_mentions_inside_comments_and_strings' panicked at crates/a2ctl/src/main.rs:1556:9:\n\
            assertion failed: find_scan_marker(\"let s = \\\"// TODO: not a comment\\\";\").is_none()\n\
            test result: FAILED. 11 passed; 1 failed";

        let focus = verification_failure_focus(note, 600).expect("focus should find failures");

        assert!(focus.contains("tests::ignores_non_task_mentions_inside_comments_and_strings"));
        assert!(focus.contains("assertion failed: find_scan_marker"));
        assert!(focus.contains("test result: FAILED"));
    }

    #[tokio::test]
    async fn worktree_verifications_are_persisted_into_lineage() {
        let verification = ExternalVerification {
            passed: false,
            command: "cargo test -p a2ctl hidden_case".into(),
            exit_code: Some(101),
            failing_tests: vec!["tests::hidden_case".into()],
            failure_focus: vec!["thread panicked at crates/a2ctl/src/main.rs:42".into()],
            stdout_excerpt: "test tests::hidden_case ... FAILED".into(),
            stderr_excerpt: "error: test failed".into(),
            verified_at: Utc::now(),
        };
        let task_id = TaskId::new();
        let config = WorkcellConfig {
            workcell_id: WorkcellId::new(),
            germline_version: GermlineVersion::new(),
            task: TaskContract {
                id: task_id,
                title: "test verifier lineage".into(),
                description: "preserve verifier output".into(),
                acceptance_criteria: vec![],
                verification_commands: vec![TaskVerificationCommand {
                    command: verification.command.clone(),
                    expect_exit: 0,
                }],
                budget: Budget {
                    max_tokens: 10_000,
                    max_duration_secs: 60,
                    max_calls: 10,
                },
                priority: Priority::Normal,
                source: TaskSource::External {
                    origin: "test".into(),
                },
                no_external_solution_search: false,
                network_policy: None,
                created_at: Utc::now(),
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![],
            enable_anti_repeat_retry: true,
        };
        let catalyst = VerifierCatalyst {
            id: CatalystId::new(),
            verification: verification.clone(),
        };

        let result = run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        assert_eq!(result.lineage.external_verifications, vec![verification]);
    }

    #[tokio::test]
    async fn no_candidate_catalyst_failure_is_persisted_as_redacted_external_verification() {
        let task_id = TaskId::new();
        let config = test_workcell_config(test_task(task_id));
        let catalyst = FailingCatalyst {
            id: CatalystId::new(),
            message: "provider failed while processing Senior SWE Bench metadata for https://private-benchmark.example.invalid/tasks/synthetic-redaction-fixture with problem_statement details".into(),
        };

        let result = run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        assert!(result.patch.is_none());
        assert_eq!(result.lineage.external_verifications.len(), 1);
        let verification = &result.lineage.external_verifications[0];
        assert!(!verification.passed);
        assert_eq!(verification.command, "catalyst.execute");
        assert_eq!(verification.exit_code, None);
        let focus = verification.failure_focus.join("\n");
        assert!(focus.contains("failed before producing a candidate patch"));
        assert!(focus.contains("withheld"));
        assert!(!focus.contains("synthetic-redaction-fixture"));
        assert!(!focus.contains("problem_statement"));
        assert!(!focus.contains("private-benchmark.example.invalid/tasks"));
    }

    #[tokio::test]
    async fn non_benchmark_no_candidate_failure_preserves_benign_url_context() {
        let task_id = TaskId::new();
        let config = test_workcell_config(test_task(task_id));
        let catalyst = FailingCatalyst {
            id: CatalystId::new(),
            message: "provider failed while reading benign docs at https://docs.example.invalid/retry-guide".into(),
        };

        let result = run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        assert!(result.patch.is_none());
        assert_eq!(result.lineage.external_verifications.len(), 1);
        let focus = result.lineage.external_verifications[0]
            .failure_focus
            .join("\n");
        assert!(focus.contains("https://docs.example.invalid/retry-guide"));
        assert!(!focus.contains("withheld"));
    }

    #[tokio::test]
    async fn benchmark_no_candidate_failure_redacts_task_specific_error_text_without_markers() {
        let task_id = TaskId::new();
        let mut task = test_task(task_id);
        task.title = "synthetic-redaction-fixture".into();
        task.source = TaskSource::External {
            origin: "senior-swe-bench".into(),
        };
        task.no_external_solution_search = true;
        let config = test_workcell_config(task);
        let catalyst = FailingCatalyst {
            id: CatalystId::new(),
            message: "provider failed while handling synthetic-redaction-fixture".into(),
        };

        let result = run_workcell(config, &catalyst, &NoopProvider, &AlwaysPassEvaluator)
            .await
            .unwrap();

        assert!(result.patch.is_none());
        assert_eq!(result.lineage.external_verifications.len(), 1);
        let focus = result.lineage.external_verifications[0]
            .failure_focus
            .join("\n");
        assert!(focus.contains("withheld"));
        assert!(!focus.contains("synthetic-redaction-fixture"));

        let retry_context = render_prior_motif(&result.lineage, 0);
        assert!(retry_context.contains("command: catalyst.execute"));
        assert!(retry_context.contains("withheld"));
        assert!(!retry_context.contains("synthetic-redaction-fixture"));
    }

    #[test]
    fn no_candidate_catalyst_failure_renders_as_retry_context() {
        let task_id = TaskId::new();
        let prior = LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id: PatchId::new(),
            patch_diff: None,
            patch_rationale: None,
            external_verifications: vec![ExternalVerification {
                passed: false,
                command: "catalyst.execute".into(),
                exit_code: None,
                failing_tests: vec![],
                failure_focus: vec![
                    "catalyst execution failed before producing a candidate patch".into(),
                ],
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
                verified_at: Utc::now(),
            }],
            parent_germline: GermlineVersion::new(),
            model_attributions: vec![],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id,
                somatic: SomaticFitness {
                    task_completed: false,
                    tests_pass: false,
                    acceptance_met: vec![],
                    tokens_used: 0,
                    duration_secs: 5.0,
                },
                germline: None,
                organizational: None,
                evaluated_at: Utc::now(),
            },
            created_at: Utc::now(),
        };

        let motif = render_prior_motif(&prior, 0);

        assert!(motif.contains("external_verification:"));
        assert!(motif.contains("command: catalyst.execute"));
        assert!(motif.contains("failed before producing a candidate patch"));
        assert!(!motif.contains("problem_statement"));
        assert!(!motif.contains("private-benchmark.example.invalid/tasks"));
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
                no_external_solution_search: false,
                network_policy: None,
                created_at: Utc::now(),
            },
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 10,
            },
            prior_lineage: vec![],
            enable_anti_repeat_retry: true,
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
