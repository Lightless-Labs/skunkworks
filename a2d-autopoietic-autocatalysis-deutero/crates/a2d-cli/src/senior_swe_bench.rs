//! Senior SWE-Bench catalog parsing and task-context policy helpers.
//!
//! The benchmark task listing is currently published as a Next.js/RSC page.
//! A²D only needs the public task metadata at this layer; coding agents must
//! receive sanitized task context and an explicit no-solution-search policy.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SENIOR_SWE_BENCH_AUDIT_SCHEMA: &str = "a2d.senior-swe-bench-audit.v1";
pub const SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA: &str = "a2d.senior-swe-bench-task-package.v1";
pub const SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA: &str =
    "a2d.senior-swe-bench-local-evaluation.v1";
pub const SENIOR_SWE_BENCH_OFFICIAL_EVALUATOR_MANIFEST_SCHEMA: &str =
    "a2d.senior-swe-bench-official-evaluator-manifest.v1";
pub const SENIOR_SWE_BENCH_CYCLE_RETRY_PLAN_SCHEMA: &str =
    "a2d.senior-swe-bench-cycle-retry-plan.v1";
pub const SENIOR_SWE_BENCH_CYCLE_RETRY_STEP_SCHEMA: &str =
    "a2d.senior-swe-bench-cycle-retry-step.v1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeniorSweBenchTask {
    pub family: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub repo_slug: String,
    #[serde(default)]
    pub task_type: String,
    #[serde(default)]
    pub segment: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub in_benchmark: bool,
    #[serde(default)]
    pub in_sample: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub taxonomy: SeniorSweBenchTaxonomy,
    #[serde(default)]
    pub environment: SeniorSweBenchEnvironment,
    #[serde(default)]
    pub hard: Option<SeniorSweBenchVariant>,
    #[serde(default)]
    pub guided: Option<SeniorSweBenchVariant>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeniorSweBenchTaxonomy {
    #[serde(default)]
    pub task_type: String,
    #[serde(default)]
    pub stack: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub skill_breadth: String,
    #[serde(default)]
    pub runtime_dependence: String,
    #[serde(default)]
    pub misdirection: String,
    #[serde(default)]
    pub estimated_human_time: String,
    #[serde(default)]
    pub oracle_scope: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeniorSweBenchEnvironment {
    #[serde(default)]
    pub cpus: Option<u64>,
    #[serde(default)]
    pub memory: String,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeniorSweBenchVariant {
    pub task_id: String,
    #[serde(default)]
    pub difficulty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeniorSweBenchAudit {
    pub schema_version: &'static str,
    pub source: String,
    pub task_count: usize,
    pub benchmark_task_count: usize,
    pub sample_task_count: usize,
    pub hard_task_count: usize,
    pub guided_task_count: usize,
    pub repo_count: usize,
    pub repos: Vec<String>,
    pub task_types: BTreeMap<String, usize>,
    pub segments: BTreeMap<String, usize>,
    pub stacks: BTreeMap<String, usize>,
    pub difficulties: BTreeMap<String, usize>,
    pub hidden_holdout_applicability: &'static str,
    pub agent_restrictions: SeniorSweBenchAgentRestrictions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeniorSweBenchAgentRestrictions {
    pub github_solution_search_allowed: bool,
    pub allowed_github_use: &'static str,
    pub forbidden_actions: Vec<&'static str>,
    pub required_agent_preamble: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeniorSweBenchTaskPackage {
    pub schema_version: &'static str,
    pub task_id: String,
    pub family: String,
    pub repo: String,
    pub segment: String,
    pub task_type: String,
    pub tags: Vec<String>,
    pub in_benchmark: bool,
    pub in_sample: bool,
    pub version: String,
    pub variant: String,
    pub difficulty: String,
    pub description: String,
    pub taxonomy: SeniorSweBenchTaxonomy,
    pub environment: SeniorSweBenchEnvironment,
    pub agent_restrictions: SeniorSweBenchAgentRestrictions,
    pub coding_agent_context: String,
    pub evaluation: SeniorSweBenchEvaluationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeniorSweBenchEvaluationStatus {
    pub status: &'static str,
    pub evaluator: &'static str,
    pub fitness: Option<String>,
    pub note: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeniorSweBenchTaskPackageSummary {
    pub task_id: String,
    pub repo: String,
    pub github_solution_search_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeniorSweBenchOfficialEvaluatorManifestSummary {
    pub benchmark_url: String,
    pub task_id: String,
    pub repo: String,
    pub hidden_holdouts: bool,
    pub github_solution_search_allowed: bool,
    pub benchmark_provided_command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeniorSweBenchLocalEvaluation {
    pub schema_version: &'static str,
    pub task_id: String,
    pub repo: String,
    pub evaluator: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub candidate_patch: String,
    pub candidate_patch_hash: String,
    pub checkout: String,
    pub evaluator_checkout: String,
    pub candidate_patch_applied: bool,
    pub evaluator_checkout_mode: String,
    pub original_checkout_mutated: bool,
    pub candidate_patch_preflight_checked: bool,
    pub candidate_patch_preflight_status: String,
    pub candidate_patch_preflight_command: String,
    pub source_revision: String,
    pub source_tree_dirty: bool,
    pub source_diff_scope: String,
    pub source_diff_hash: String,
    pub evidence_command: String,
    pub evaluator_command: Vec<String>,
    pub github_solution_search_allowed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_evaluator_manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_evaluator_manifest_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_benchmark_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_hidden_holdouts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_github_solution_search_allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_benchmark_provided_command: Option<Vec<String>>,
    pub stdout_preview: String,
    pub stderr_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness_evidence_path: Option<String>,
    pub note: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeniorSweBenchError {
    MissingTasksArray,
    UnterminatedTasksArray,
    InvalidTasksJson(String),
}

impl fmt::Display for SeniorSweBenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeniorSweBenchError::MissingTasksArray => {
                write!(f, "Senior SWE-Bench tasks array not found")
            }
            SeniorSweBenchError::UnterminatedTasksArray => {
                write!(f, "Senior SWE-Bench tasks array is unterminated")
            }
            SeniorSweBenchError::InvalidTasksJson(error) => {
                write!(f, "invalid Senior SWE-Bench tasks JSON: {error}")
            }
        }
    }
}

impl std::error::Error for SeniorSweBenchError {}

pub fn parse_senior_swe_bench_task_package(
    input: &str,
) -> Result<SeniorSweBenchTaskPackageSummary, String> {
    let value: serde_json::Value = serde_json::from_str(input)
        .map_err(|error| format!("invalid Senior SWE-Bench task package JSON: {error}"))?;
    let schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench task package missing schema_version".to_string())?;
    if schema != SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA {
        return Err(format!(
            "expected {SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA}, got {schema}"
        ));
    }
    let task_id = required_string(&value, "task_id")?;
    let repo = required_string(&value, "repo")?;
    let github_solution_search_allowed = value
        .get("agent_restrictions")
        .and_then(|restrictions| restrictions.get("github_solution_search_allowed"))
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            "Senior SWE-Bench task package missing agent_restrictions.github_solution_search_allowed"
                .to_string()
        })?;
    ensure_solution_search_forbidden(github_solution_search_allowed, "task package")?;
    Ok(SeniorSweBenchTaskPackageSummary {
        task_id,
        repo,
        github_solution_search_allowed,
    })
}

pub fn parse_senior_swe_bench_cycle_input(
    input: &str,
) -> Result<SeniorSweBenchTaskPackageSummary, String> {
    let value: serde_json::Value = serde_json::from_str(input)
        .map_err(|error| format!("invalid Senior SWE-Bench cycle input JSON: {error}"))?;
    let context = value
        .get("benchmark_context")
        .ok_or_else(|| "Senior SWE-Bench cycle input missing benchmark_context".to_string())?;
    let schema = context
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench cycle input missing benchmark_context.schema_version".to_string()
        })?;
    if schema != SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA {
        return Err(format!(
            "expected benchmark_context.schema_version {SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA}, got {schema}"
        ));
    }
    let task_id = required_string(context, "benchmark_context.task_id")?;
    let repo = required_string(context, "benchmark_context.repo")?;
    let github_solution_search_allowed = context
        .get("github_solution_search_allowed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            "Senior SWE-Bench cycle input missing benchmark_context.github_solution_search_allowed"
                .to_string()
        })?;
    ensure_solution_search_forbidden(github_solution_search_allowed, "cycle input")?;

    let evaluation = value
        .get("evaluation")
        .ok_or_else(|| "Senior SWE-Bench cycle input missing evaluation".to_string())?;
    let status = evaluation
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench cycle input missing evaluation.status".to_string())?;
    if status != "not_evaluated" {
        return Err(format!(
            "Senior SWE-Bench cycle input evaluation.status must be not_evaluated, got {status}"
        ));
    }
    if !evaluation
        .get("fitness")
        .is_some_and(serde_json::Value::is_null)
    {
        return Err(
            "Senior SWE-Bench cycle input evaluation.fitness must be null before evaluation"
                .to_string(),
        );
    }

    Ok(SeniorSweBenchTaskPackageSummary {
        task_id,
        repo,
        github_solution_search_allowed,
    })
}

pub fn parse_senior_swe_bench_official_evaluator_manifest(
    input: &str,
    package: &SeniorSweBenchTaskPackageSummary,
    invoked_command: &[String],
) -> Result<SeniorSweBenchOfficialEvaluatorManifestSummary, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|error| {
        format!("invalid Senior SWE-Bench official evaluator manifest JSON: {error}")
    })?;
    let schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench official evaluator manifest missing schema_version".to_string()
        })?;
    if schema != SENIOR_SWE_BENCH_OFFICIAL_EVALUATOR_MANIFEST_SCHEMA {
        return Err(format!(
            "expected {SENIOR_SWE_BENCH_OFFICIAL_EVALUATOR_MANIFEST_SCHEMA}, got {schema}"
        ));
    }
    let benchmark_url = required_string(&value, "benchmark_url")?;
    if !benchmark_url.contains("senior-swe-bench.snorkel.ai") {
        return Err(format!(
            "Senior SWE-Bench official evaluator manifest benchmark_url is not Senior SWE-Bench: {benchmark_url}"
        ));
    }
    let task_id = required_string(&value, "task_id")?;
    if task_id != package.task_id {
        return Err(format!(
            "Senior SWE-Bench official evaluator manifest task_id {task_id} does not match task input {}",
            package.task_id
        ));
    }
    let repo = required_string(&value, "repo")?;
    if repo != package.repo {
        return Err(format!(
            "Senior SWE-Bench official evaluator manifest repo {repo} does not match task input {}",
            package.repo
        ));
    }
    let hidden_holdouts = value
        .get("hidden_holdouts")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            "Senior SWE-Bench official evaluator manifest missing hidden_holdouts".to_string()
        })?;
    if !hidden_holdouts {
        return Err(
            "Senior SWE-Bench official evaluator manifest must declare hidden_holdouts: true"
                .to_string(),
        );
    }
    let github_solution_search_allowed = value
        .get("github_solution_search_allowed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            "Senior SWE-Bench official evaluator manifest missing github_solution_search_allowed"
                .to_string()
        })?;
    ensure_solution_search_forbidden(
        github_solution_search_allowed,
        "official evaluator manifest",
    )?;
    let benchmark_provided_command = value
        .get("benchmark_provided_command")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench official evaluator manifest missing benchmark_provided_command"
                .to_string()
        })?
        .iter()
        .map(|part| {
            part.as_str()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    "Senior SWE-Bench official evaluator manifest benchmark_provided_command contains non-string entry"
                        .to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if benchmark_provided_command.is_empty() {
        return Err(
            "Senior SWE-Bench official evaluator manifest benchmark_provided_command is empty"
                .to_string(),
        );
    }
    if benchmark_provided_command != invoked_command {
        return Err(
            "Senior SWE-Bench official evaluator manifest benchmark_provided_command does not match invoked evaluator command"
                .to_string(),
        );
    }
    Ok(SeniorSweBenchOfficialEvaluatorManifestSummary {
        benchmark_url,
        task_id,
        repo,
        hidden_holdouts,
        github_solution_search_allowed,
        benchmark_provided_command,
    })
}

pub fn build_senior_swe_bench_local_evaluation(
    package: &SeniorSweBenchTaskPackageSummary,
    evaluator: impl Into<String>,
    status: impl Into<String>,
    exit_code: Option<i32>,
    candidate_patch: impl Into<String>,
    candidate_patch_hash: impl Into<String>,
    checkout: impl Into<String>,
    evaluator_checkout: impl Into<String>,
    candidate_patch_applied: bool,
    evaluator_checkout_mode: impl Into<String>,
    original_checkout_mutated: bool,
    candidate_patch_preflight_checked: bool,
    candidate_patch_preflight_status: impl Into<String>,
    candidate_patch_preflight_command: impl Into<String>,
    source_revision: impl Into<String>,
    source_tree_dirty: bool,
    source_diff_scope: impl Into<String>,
    source_diff_hash: impl Into<String>,
    evidence_command: impl Into<String>,
    evaluator_command: Vec<String>,
    official_evaluator_manifest_path: Option<String>,
    official_evaluator_manifest_hash: Option<String>,
    official_manifest: Option<&SeniorSweBenchOfficialEvaluatorManifestSummary>,
    stdout: &str,
    stderr: &str,
    fitness_evidence_path: Option<String>,
) -> SeniorSweBenchLocalEvaluation {
    SeniorSweBenchLocalEvaluation {
        schema_version: SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
        task_id: package.task_id.clone(),
        repo: package.repo.clone(),
        evaluator: evaluator.into(),
        status: status.into(),
        exit_code,
        candidate_patch: candidate_patch.into(),
        candidate_patch_hash: candidate_patch_hash.into(),
        checkout: checkout.into(),
        evaluator_checkout: evaluator_checkout.into(),
        candidate_patch_applied,
        evaluator_checkout_mode: evaluator_checkout_mode.into(),
        original_checkout_mutated,
        candidate_patch_preflight_checked,
        candidate_patch_preflight_status: candidate_patch_preflight_status.into(),
        candidate_patch_preflight_command: candidate_patch_preflight_command.into(),
        source_revision: source_revision.into(),
        source_tree_dirty,
        source_diff_scope: source_diff_scope.into(),
        source_diff_hash: source_diff_hash.into(),
        evidence_command: evidence_command.into(),
        evaluator_command,
        github_solution_search_allowed: package.github_solution_search_allowed,
        official_evaluator_manifest_path,
        official_evaluator_manifest_hash,
        official_benchmark_url: official_manifest.map(|manifest| manifest.benchmark_url.clone()),
        official_task_id: official_manifest.map(|manifest| manifest.task_id.clone()),
        official_repo: official_manifest.map(|manifest| manifest.repo.clone()),
        official_hidden_holdouts: official_manifest.map(|manifest| manifest.hidden_holdouts),
        official_github_solution_search_allowed: official_manifest
            .map(|manifest| manifest.github_solution_search_allowed),
        official_benchmark_provided_command: official_manifest
            .map(|manifest| manifest.benchmark_provided_command.clone()),
        stdout_preview: preview_text(stdout),
        stderr_preview: preview_text(stderr),
        fitness_evidence_path,
        note: "local evaluator wrapper only; claim official Senior SWE-Bench fitness only when this command runs the benchmark-provided official evaluator/holdouts",
    }
}

pub fn extract_senior_swe_bench_tasks(
    input: &str,
) -> Result<Vec<SeniorSweBenchTask>, SeniorSweBenchError> {
    let decoded = decode_next_rsc_escaped_json(input);
    let key = "\"tasks\":";
    let key_index = decoded
        .find(key)
        .ok_or(SeniorSweBenchError::MissingTasksArray)?;
    let search_start = key_index + key.len();
    let array_start = decoded[search_start..]
        .find('[')
        .map(|offset| search_start + offset)
        .ok_or(SeniorSweBenchError::MissingTasksArray)?;
    let array_end = find_json_array_end(&decoded, array_start)
        .ok_or(SeniorSweBenchError::UnterminatedTasksArray)?;
    serde_json::from_str(&decoded[array_start..array_end])
        .map_err(|error| SeniorSweBenchError::InvalidTasksJson(error.to_string()))
}

pub fn senior_swe_bench_agent_restrictions() -> SeniorSweBenchAgentRestrictions {
    SeniorSweBenchAgentRestrictions {
        github_solution_search_allowed: false,
        allowed_github_use: "Use only a benchmark-provided repository checkout or harness. Do not use GitHub search, issues, pull requests, commits, forks, or public web pages to look for the benchmark solution.",
        forbidden_actions: vec![
            "searching GitHub for task IDs, issue titles, descriptions, commits, pull requests, forks, or patches",
            "querying GitHub search APIs or public code search for solution-bearing terms",
            "opening upstream issue/PR/commit pages to discover the benchmark fix",
            "copying solution patches from public repositories or benchmark dataset discussions",
        ],
        required_agent_preamble: "Senior SWE-Bench policy: solve from the provided task text, local checkout, and local tests only. Do not search GitHub or the public web for task IDs, issues, PRs, commits, forks, or solution patches.",
    }
}

pub fn build_senior_swe_bench_audit(
    tasks: &[SeniorSweBenchTask],
    source: impl Into<String>,
) -> SeniorSweBenchAudit {
    let mut repos = BTreeSet::new();
    let mut task_types = BTreeMap::new();
    let mut segments = BTreeMap::new();
    let mut stacks = BTreeMap::new();
    let mut difficulties = BTreeMap::new();

    for task in tasks {
        let repo_id = repository_id(task);
        if repo_id != "unknown" {
            repos.insert(repo_id);
        }
        bump(&mut task_types, non_empty(&task.task_type, "unknown"));
        bump(&mut segments, non_empty(&task.segment, "unknown"));
        for stack in &task.taxonomy.stack {
            bump(&mut stacks, non_empty(stack, "unknown"));
        }
        if let Some(hard) = &task.hard {
            bump(
                &mut difficulties,
                format!("hard:{}", non_empty(&hard.difficulty, "unknown")),
            );
        }
        if let Some(guided) = &task.guided {
            bump(
                &mut difficulties,
                format!("guided:{}", non_empty(&guided.difficulty, "unknown")),
            );
        }
    }

    SeniorSweBenchAudit {
        schema_version: SENIOR_SWE_BENCH_AUDIT_SCHEMA,
        source: source.into(),
        task_count: tasks.len(),
        benchmark_task_count: tasks.iter().filter(|task| task.in_benchmark).count(),
        sample_task_count: tasks.iter().filter(|task| task.in_sample).count(),
        hard_task_count: tasks.iter().filter(|task| task.hard.is_some()).count(),
        guided_task_count: tasks.iter().filter(|task| task.guided.is_some()).count(),
        repo_count: repos.len(),
        repos: repos.into_iter().collect(),
        task_types,
        segments,
        stacks,
        difficulties,
        hidden_holdout_applicability: "catalog-audit-only: no candidate patch is evaluated here; use the official Senior SWE-Bench evaluator/holdouts for task fitness",
        agent_restrictions: senior_swe_bench_agent_restrictions(),
    }
}

pub fn build_senior_swe_bench_task_package(
    task: &SeniorSweBenchTask,
    variant_name: &str,
    variant: &SeniorSweBenchVariant,
) -> SeniorSweBenchTaskPackage {
    SeniorSweBenchTaskPackage {
        schema_version: SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA,
        task_id: variant.task_id.clone(),
        family: task.family.clone(),
        repo: repository_id(task),
        segment: task.segment.clone(),
        task_type: task.task_type.clone(),
        tags: task.tags.clone(),
        in_benchmark: task.in_benchmark,
        in_sample: task.in_sample,
        version: task.version.clone(),
        variant: variant_name.to_string(),
        difficulty: variant.difficulty.clone(),
        description: task.description.clone(),
        taxonomy: task.taxonomy.clone(),
        environment: task.environment.clone(),
        agent_restrictions: senior_swe_bench_agent_restrictions(),
        coding_agent_context: render_senior_swe_bench_task_context(task, variant),
        evaluation: SeniorSweBenchEvaluationStatus {
            status: "not_evaluated",
            evaluator: "official_senior_swe_bench",
            fitness: None,
            note: "run the official Senior SWE-Bench evaluator/holdouts against a candidate patch before claiming task fitness",
        },
    }
}

pub fn build_senior_swe_bench_cycle_input(
    task: &SeniorSweBenchTask,
    variant_name: &str,
    variant: &SeniorSweBenchVariant,
) -> serde_json::Value {
    let context = render_senior_swe_bench_task_context(task, variant);
    let restrictions = senior_swe_bench_agent_restrictions();
    serde_json::json!({
        "requirements": format!(
            "{context}\n\nDeliverable: produce a unified diff candidate patch for the benchmark-provided checkout. Do not output a full replacement repository. Do not claim task fitness until an evaluator runs."
        ),
        "design": format!(
            "Work only inside the supplied local checkout for {repo}. Use local source inspection and local tests/harnesses. Public GitHub/web solution search is forbidden. The expected artifact is a candidate patch diff, not an A²D source patch.",
            repo = repository_id(task)
        ),
        "plan": "1. Inspect the provided checkout and task context.\n2. Identify the failing behavior using local tests or a local harness only.\n3. Implement the smallest candidate patch.\n4. Run relevant local tests.\n5. Return only a unified diff suitable for the evaluator wrapper.".to_string(),
        "benchmark_context": {
            "schema_version": SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA,
            "task_id": variant.task_id,
            "repo": repository_id(task),
            "family": task.family,
            "variant": variant_name,
            "difficulty": variant.difficulty,
            "in_benchmark": task.in_benchmark,
            "in_sample": task.in_sample,
            "github_solution_search_allowed": restrictions.github_solution_search_allowed,
        },
        "evaluation": {
            "status": "not_evaluated",
            "evaluator": "official_senior_swe_bench",
            "fitness": serde_json::Value::Null,
            "note": "run a local or official Senior SWE-Bench evaluator against the candidate patch before claiming task fitness"
        }
    })
}

pub fn build_senior_swe_bench_cycle_retry_plan(
    cycle_input: &str,
    max_attempts: usize,
) -> Result<serde_json::Value, String> {
    if max_attempts == 0 {
        return Err(
            "Senior SWE-Bench cycle retry plan max_attempts must be greater than zero".to_string(),
        );
    }
    if max_attempts > 8 {
        return Err(
            "Senior SWE-Bench cycle retry plan max_attempts must be <= 8 for bounded execution"
                .to_string(),
        );
    }
    let package = parse_senior_swe_bench_cycle_input(cycle_input)?;
    let cycle_value: serde_json::Value = serde_json::from_str(cycle_input)
        .map_err(|error| format!("invalid Senior SWE-Bench cycle input JSON: {error}"))?;
    reject_reserved_cycle_feedback_artifacts(&cycle_value)?;
    reject_public_solution_refs_in_cycle_feedback_content(&cycle_value)?;

    let attempts = (0..max_attempts)
        .map(|attempt| {
            serde_json::json!({
                "attempt_index": attempt,
                "cycle_input_source": if attempt == 0 { "initial_task_cycle_input" } else { "feedback_from_previous_local_evaluation" },
                "required_gates": [
                    "run_cycle_input_with_output_artifacts",
                    "extract_unified_diff_candidate_patch",
                    "evaluate_candidate_patch_against_checkout",
                    "inspect_a2d_fitness_evidence_when_evaluator_passes"
                ],
                "on_patch_extraction_failure": "stop_fail_closed_without_evaluator_or_fitness_claim",
                "on_evaluation_passed": "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
                "on_evaluation_failed": if attempt + 1 < max_attempts {
                    "build_next_cycle_input_with_senior_swe_bench_cycle_input_feedback"
                } else {
                    "stop_attempts_exhausted_without_fitness_claim"
                }
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "schema_version": SENIOR_SWE_BENCH_CYCLE_RETRY_PLAN_SCHEMA,
        "task_id": package.task_id,
        "repo": package.repo,
        "github_solution_search_allowed": false,
        "max_attempts": max_attempts,
        "provider_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "success_requires": [
            "a2d.fitness-evidence.v1",
            "actual_tests_evaluated:true",
            "non_regressing:true",
            "all_tests_pass:true",
            "Senior SWE-Bench official mastery additionally requires official_senior_swe_bench evaluator_kind and manifest provenance"
        ],
        "stop_criteria": [
            "candidate_patch_extraction_failed",
            "evaluation_passed_with_valid_fitness_evidence",
            "evaluation_rejected_for_policy_or_binding_mismatch",
            "max_attempts_exhausted"
        ],
        "information_barriers": {
            "public_github_solution_search_allowed": false,
            "official_hidden_holdout_output_to_coder": "redacted",
            "local_evaluator_output_to_coder": "only_when_feedback_visibility_is_public_local_test_output",
            "runtime_artifacts_seeded_from_cycle_input": false
        },
        "attempts": attempts,
        "note": "planning/validation artifact only: this command starts no providers, runs no evaluator, and is not fitness evidence"
    }))
}

pub fn build_senior_swe_bench_cycle_retry_step(
    retry_plan: &str,
    attempt_index: usize,
    cycle_input: &str,
    local_evaluation: &str,
) -> Result<serde_json::Value, String> {
    let retry_plan_value: serde_json::Value = serde_json::from_str(retry_plan)
        .map_err(|error| format!("invalid Senior SWE-Bench retry plan JSON: {error}"))?;
    validate_senior_swe_bench_retry_plan_for_step(&retry_plan_value, attempt_index)?;
    let package = parse_senior_swe_bench_cycle_input(cycle_input)?;
    let cycle_value: serde_json::Value = serde_json::from_str(cycle_input)
        .map_err(|error| format!("invalid Senior SWE-Bench cycle input JSON: {error}"))?;
    reject_reserved_cycle_feedback_artifacts(&cycle_value)?;
    reject_public_solution_refs_in_cycle_feedback_content(&cycle_value)?;
    let plan_task_id = safe_benchmark_identifier(
        "task_id",
        &required_string(&retry_plan_value, "task_id")?,
        false,
    )?;
    let plan_repo =
        safe_benchmark_identifier("repo", &required_string(&retry_plan_value, "repo")?, true)?;
    let task_id = safe_benchmark_identifier("task_id", &package.task_id, false)?;
    let repo = safe_benchmark_identifier("repo", &package.repo, true)?;
    if plan_task_id != task_id {
        return Err(format!(
            "Senior SWE-Bench retry plan task_id {plan_task_id} does not match cycle input {task_id}"
        ));
    }
    if plan_repo != repo {
        return Err(format!(
            "Senior SWE-Bench retry plan repo {plan_repo} does not match cycle input {repo}"
        ));
    }

    let evaluation_value: serde_json::Value = serde_json::from_str(local_evaluation)
        .map_err(|error| format!("invalid Senior SWE-Bench local evaluation JSON: {error}"))?;
    reject_public_solution_refs_in_cycle_feedback_content_at(&evaluation_value, "$")?;
    let schema = evaluation_value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench local evaluation missing schema_version".to_string())?;
    if schema != SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA {
        return Err(format!(
            "expected {SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA}, got {schema}"
        ));
    }
    let evaluation_task_id = safe_benchmark_identifier(
        "task_id",
        &required_string(&evaluation_value, "task_id")?,
        false,
    )?;
    if evaluation_task_id != task_id {
        return Err(format!(
            "Senior SWE-Bench local evaluation task_id {evaluation_task_id} does not match cycle input {task_id}"
        ));
    }
    let evaluation_repo =
        safe_benchmark_identifier("repo", &required_string(&evaluation_value, "repo")?, true)?;
    if evaluation_repo != repo {
        return Err(format!(
            "Senior SWE-Bench local evaluation repo {evaluation_repo} does not match cycle input {repo}"
        ));
    }
    ensure_solution_search_forbidden(
        evaluation_value
            .get("github_solution_search_allowed")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                "Senior SWE-Bench local evaluation missing github_solution_search_allowed"
                    .to_string()
            })?,
        "local evaluation",
    )?;
    let status = safe_evaluation_status(&required_string(&evaluation_value, "status")?)?;
    let _evaluator = safe_evaluator_kind(&required_string(&evaluation_value, "evaluator")?)?;
    let _candidate_patch_hash =
        safe_candidate_patch_hash(&required_string(&evaluation_value, "candidate_patch_hash")?)?;
    let is_final_attempt = attempt_index + 1
        >= retry_plan_value
            .get("max_attempts")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "Senior SWE-Bench retry plan missing max_attempts".to_string())?
            as usize;

    let mut step = serde_json::json!({
        "schema_version": SENIOR_SWE_BENCH_CYCLE_RETRY_STEP_SCHEMA,
        "task_id": task_id,
        "repo": repo,
        "attempt_index": attempt_index,
        "max_attempts": retry_plan_value["max_attempts"],
        "evaluation_status": status,
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "github_solution_search_allowed": false,
        "note": "deterministic retry-step decision only: this command starts no providers, runs no evaluator, and is not fitness evidence"
    });

    match status.as_str() {
        "failed" if !is_final_attempt => {
            step["decision"] = serde_json::Value::String("build_next_cycle_input".to_string());
            step["next_step"] = serde_json::Value::String(
                "run a2d cycle-input with the included next_cycle_input, capture output artifacts, then extract/evaluate the candidate patch".to_string(),
            );
            step["next_cycle_input"] =
                build_senior_swe_bench_cycle_input_feedback(cycle_input, local_evaluation)?;
        }
        "failed" => {
            step["decision"] = serde_json::Value::String("stop".to_string());
            step["stop_reason"] = serde_json::Value::String("max_attempts_exhausted".to_string());
        }
        "passed" => {
            let Some(path) = evaluation_value
                .get("fitness_evidence_path")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
            else {
                step["decision"] = serde_json::Value::String("stop".to_string());
                step["stop_reason"] =
                    serde_json::Value::String("missing_fitness_evidence_path".to_string());
                return Ok(step);
            };
            let path = safe_feedback_atom("fitness_evidence_path", path)?;
            step["decision"] = serde_json::Value::String("inspect_fitness_evidence".to_string());
            step["fitness_evidence_path"] = serde_json::Value::String(path.clone());
            step["fitness_evidence_inspect_args"] =
                serde_json::json!(["fitness-evidence-inspect", path, "--require-all-tests-pass"]);
            step["next_step"] = serde_json::Value::String(
                "run the suggested fitness-evidence-inspect command before claiming task fitness"
                    .to_string(),
            );
        }
        _ => unreachable!("safe_evaluation_status only returns reviewed statuses"),
    }

    Ok(step)
}

pub fn validate_senior_swe_bench_cycle_retry_plan_step(
    retry_plan: &str,
    attempt_index: usize,
) -> Result<serde_json::Value, String> {
    let retry_plan_value: serde_json::Value = serde_json::from_str(retry_plan)
        .map_err(|error| format!("invalid Senior SWE-Bench retry plan JSON: {error}"))?;
    validate_senior_swe_bench_retry_plan_for_step(&retry_plan_value, attempt_index)?;
    Ok(retry_plan_value)
}

pub fn validate_senior_swe_bench_retry_plan_and_cycle_input_for_attempt(
    retry_plan: &str,
    attempt_index: usize,
    cycle_input: &str,
) -> Result<(serde_json::Value, SeniorSweBenchTaskPackageSummary), String> {
    let retry_plan_value =
        validate_senior_swe_bench_cycle_retry_plan_step(retry_plan, attempt_index)?;
    let package = parse_senior_swe_bench_cycle_input(cycle_input)?;
    let cycle_value: serde_json::Value = serde_json::from_str(cycle_input)
        .map_err(|error| format!("invalid Senior SWE-Bench cycle input JSON: {error}"))?;
    reject_reserved_cycle_feedback_artifacts(&cycle_value)?;
    reject_public_solution_refs_in_cycle_feedback_content(&cycle_value)?;
    let plan_task_id = safe_benchmark_identifier(
        "task_id",
        &required_string(&retry_plan_value, "task_id")?,
        false,
    )?;
    let plan_repo =
        safe_benchmark_identifier("repo", &required_string(&retry_plan_value, "repo")?, true)?;
    let task_id = safe_benchmark_identifier("task_id", &package.task_id, false)?;
    let repo = safe_benchmark_identifier("repo", &package.repo, true)?;
    if plan_task_id != task_id {
        return Err(format!(
            "Senior SWE-Bench retry plan task_id {plan_task_id} does not match cycle input {task_id}"
        ));
    }
    if plan_repo != repo {
        return Err(format!(
            "Senior SWE-Bench retry plan repo {plan_repo} does not match cycle input {repo}"
        ));
    }
    Ok((retry_plan_value, package))
}

fn validate_senior_swe_bench_retry_plan_for_step(
    retry_plan: &serde_json::Value,
    attempt_index: usize,
) -> Result<(), String> {
    let schema = retry_plan
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing schema_version".to_string())?;
    if schema != SENIOR_SWE_BENCH_CYCLE_RETRY_PLAN_SCHEMA {
        return Err(format!(
            "expected {SENIOR_SWE_BENCH_CYCLE_RETRY_PLAN_SCHEMA}, got {schema}"
        ));
    }
    safe_benchmark_identifier("task_id", &required_string(retry_plan, "task_id")?, false)?;
    safe_benchmark_identifier("repo", &required_string(retry_plan, "repo")?, true)?;
    ensure_solution_search_forbidden(
        retry_plan
            .get("github_solution_search_allowed")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                "Senior SWE-Bench retry plan missing github_solution_search_allowed".to_string()
            })?,
        "retry plan",
    )?;
    if retry_plan
        .get("provider_invocations_started")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry plan must not have started provider invocations".to_string(),
        );
    }
    if retry_plan
        .get("fitness_claim_allowed_before_evidence")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry plan must forbid fitness claims before evidence".to_string(),
        );
    }
    let max_attempts = retry_plan
        .get("max_attempts")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing max_attempts".to_string())?
        as usize;
    if !(1..=8).contains(&max_attempts) {
        return Err("Senior SWE-Bench retry plan max_attempts must be in 1..=8".to_string());
    }
    if attempt_index >= max_attempts {
        return Err(format!(
            "Senior SWE-Bench retry step attempt_index {attempt_index} is outside max_attempts {max_attempts}"
        ));
    }
    let attempts = retry_plan
        .get("attempts")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing attempts".to_string())?;
    if attempts.len() != max_attempts {
        return Err(
            "Senior SWE-Bench retry plan attempts length must match max_attempts".to_string(),
        );
    }
    validate_retry_plan_string_array(
        retry_plan,
        "success_requires",
        &[
            "a2d.fitness-evidence.v1",
            "actual_tests_evaluated:true",
            "non_regressing:true",
            "all_tests_pass:true",
        ],
    )?;
    validate_retry_plan_string_array(
        retry_plan,
        "stop_criteria",
        &[
            "candidate_patch_extraction_failed",
            "evaluation_passed_with_valid_fitness_evidence",
            "evaluation_rejected_for_policy_or_binding_mismatch",
            "max_attempts_exhausted",
        ],
    )?;
    validate_retry_plan_information_barriers(retry_plan)?;
    for (index, attempt) in attempts.iter().enumerate() {
        validate_retry_plan_attempt(attempt, index, max_attempts)?;
    }
    let indexed_attempt = attempts
        .get(attempt_index)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing indexed attempt".to_string())?;
    if indexed_attempt
        .get("attempt_index")
        .and_then(serde_json::Value::as_u64)
        != Some(attempt_index as u64)
    {
        return Err(
            "Senior SWE-Bench retry plan indexed attempt does not match attempt_index".to_string(),
        );
    }
    Ok(())
}

fn validate_retry_plan_string_array(
    retry_plan: &serde_json::Value,
    field: &str,
    required_entries: &[&str],
) -> Result<(), String> {
    let entries = retry_plan
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Senior SWE-Bench retry plan missing {field}"))?;
    if entries.is_empty() || entries.iter().any(|entry| entry.as_str().is_none()) {
        return Err(format!(
            "Senior SWE-Bench retry plan {field} must be a non-empty string array"
        ));
    }
    for required in required_entries {
        if !entries
            .iter()
            .any(|entry| entry.as_str() == Some(*required))
        {
            return Err(format!(
                "Senior SWE-Bench retry plan {field} missing required entry {required}"
            ));
        }
    }
    Ok(())
}

fn validate_retry_plan_information_barriers(retry_plan: &serde_json::Value) -> Result<(), String> {
    let barriers = retry_plan
        .get("information_barriers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing information_barriers".to_string())?;
    if barriers
        .get("public_github_solution_search_allowed")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry plan information_barriers must forbid public GitHub solution search"
                .to_string(),
        );
    }
    if barriers
        .get("official_hidden_holdout_output_to_coder")
        .and_then(serde_json::Value::as_str)
        != Some("redacted")
    {
        return Err(
            "Senior SWE-Bench retry plan information_barriers must redact official hidden holdout output"
                .to_string(),
        );
    }
    if barriers
        .get("local_evaluator_output_to_coder")
        .and_then(serde_json::Value::as_str)
        != Some("only_when_feedback_visibility_is_public_local_test_output")
    {
        return Err(
            "Senior SWE-Bench retry plan information_barriers must gate local evaluator output visibility"
                .to_string(),
        );
    }
    if barriers
        .get("runtime_artifacts_seeded_from_cycle_input")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry plan information_barriers must forbid seeded runtime artifacts"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_retry_plan_attempt(
    attempt: &serde_json::Value,
    index: usize,
    max_attempts: usize,
) -> Result<(), String> {
    if attempt
        .get("attempt_index")
        .and_then(serde_json::Value::as_u64)
        != Some(index as u64)
    {
        return Err(format!(
            "Senior SWE-Bench retry plan attempt {index} does not match attempt_index"
        ));
    }
    let expected_source = if index == 0 {
        "initial_task_cycle_input"
    } else {
        "feedback_from_previous_local_evaluation"
    };
    if attempt
        .get("cycle_input_source")
        .and_then(serde_json::Value::as_str)
        != Some(expected_source)
    {
        return Err(format!(
            "Senior SWE-Bench retry plan attempt {index} has invalid cycle_input_source"
        ));
    }
    validate_retry_plan_attempt_string_array(
        attempt,
        index,
        "required_gates",
        &[
            "run_cycle_input_with_output_artifacts",
            "extract_unified_diff_candidate_patch",
            "evaluate_candidate_patch_against_checkout",
            "inspect_a2d_fitness_evidence_when_evaluator_passes",
        ],
    )?;
    validate_retry_plan_attempt_string(
        attempt,
        index,
        "on_patch_extraction_failure",
        "stop_fail_closed_without_evaluator_or_fitness_claim",
    )?;
    validate_retry_plan_attempt_string(
        attempt,
        index,
        "on_evaluation_passed",
        "stop_success_only_after_a2d_fitness_evidence_v1_non_regressing_actual_tests",
    )?;
    validate_retry_plan_attempt_string(
        attempt,
        index,
        "on_evaluation_failed",
        if index + 1 < max_attempts {
            "build_next_cycle_input_with_senior_swe_bench_cycle_input_feedback"
        } else {
            "stop_attempts_exhausted_without_fitness_claim"
        },
    )?;
    Ok(())
}

fn validate_retry_plan_attempt_string(
    attempt: &serde_json::Value,
    index: usize,
    field: &str,
    expected: &str,
) -> Result<(), String> {
    if attempt.get(field).and_then(serde_json::Value::as_str) != Some(expected) {
        return Err(format!(
            "Senior SWE-Bench retry plan attempt {index} has invalid {field}"
        ));
    }
    Ok(())
}

fn validate_retry_plan_attempt_string_array(
    attempt: &serde_json::Value,
    index: usize,
    field: &str,
    required_entries: &[&str],
) -> Result<(), String> {
    let entries = attempt
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Senior SWE-Bench retry plan attempt {index} missing {field}"))?;
    if entries.is_empty() || entries.iter().any(|entry| entry.as_str().is_none()) {
        return Err(format!(
            "Senior SWE-Bench retry plan attempt {index} {field} must be a non-empty string array"
        ));
    }
    for required in required_entries {
        if !entries
            .iter()
            .any(|entry| entry.as_str() == Some(*required))
        {
            return Err(format!(
                "Senior SWE-Bench retry plan attempt {index} {field} missing required entry {required}"
            ));
        }
    }
    Ok(())
}

pub fn build_senior_swe_bench_cycle_input_feedback(
    cycle_input: &str,
    local_evaluation: &str,
) -> Result<serde_json::Value, String> {
    let package = parse_senior_swe_bench_cycle_input(cycle_input)?;
    let mut cycle_value: serde_json::Value = serde_json::from_str(cycle_input)
        .map_err(|error| format!("invalid Senior SWE-Bench cycle input JSON: {error}"))?;
    reject_reserved_cycle_feedback_artifacts(&cycle_value)?;
    reject_public_solution_refs_in_cycle_feedback_content(&cycle_value)?;
    let evaluation_value: serde_json::Value = serde_json::from_str(local_evaluation)
        .map_err(|error| format!("invalid Senior SWE-Bench local evaluation JSON: {error}"))?;
    let schema = evaluation_value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench local evaluation missing schema_version".to_string())?;
    if schema != SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA {
        return Err(format!(
            "expected {SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA}, got {schema}"
        ));
    }
    let task_id = safe_benchmark_identifier("task_id", &package.task_id, false)?;
    let evaluation_task_id = safe_benchmark_identifier(
        "task_id",
        &required_string(&evaluation_value, "task_id")?,
        false,
    )?;
    if evaluation_task_id != task_id {
        return Err(format!(
            "Senior SWE-Bench local evaluation task_id {evaluation_task_id} does not match cycle input {task_id}"
        ));
    }
    let repo = safe_benchmark_identifier("repo", &package.repo, true)?;
    let evaluation_repo =
        safe_benchmark_identifier("repo", &required_string(&evaluation_value, "repo")?, true)?;
    if evaluation_repo != repo {
        return Err(format!(
            "Senior SWE-Bench local evaluation repo {evaluation_repo} does not match cycle input {repo}"
        ));
    }
    let github_solution_search_allowed = evaluation_value
        .get("github_solution_search_allowed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            "Senior SWE-Bench local evaluation missing github_solution_search_allowed".to_string()
        })?;
    ensure_solution_search_forbidden(github_solution_search_allowed, "local evaluation")?;

    let status = safe_evaluation_status(&required_string(&evaluation_value, "status")?)?;
    let evaluator = safe_evaluator_kind(&required_string(&evaluation_value, "evaluator")?)?;
    let official_hidden_holdouts = match evaluation_value.get("official_hidden_holdouts") {
        Some(value) => value.as_bool().ok_or_else(|| {
            "Senior SWE-Bench local evaluation official_hidden_holdouts must be boolean".to_string()
        })?,
        None => false,
    };
    let feedback_visibility = evaluation_value
        .get("feedback_visibility")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("redacted_by_default");
    let may_show_evaluator_output = !official_hidden_holdouts
        && evaluator != "official_senior_swe_bench"
        && feedback_visibility == "public_local_test_output";
    let candidate_patch_hash =
        safe_candidate_patch_hash(&required_string(&evaluation_value, "candidate_patch_hash")?)?;
    let exit_code = evaluation_value
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
        .map(|code| code.to_string())
        .unwrap_or_else(|| "not_reported".to_string());
    let stdout_preview = feedback_preview_for_cycle_input(
        &evaluation_value,
        "stdout_preview",
        may_show_evaluator_output,
    )?;
    let stderr_preview = feedback_preview_for_cycle_input(
        &evaluation_value,
        "stderr_preview",
        may_show_evaluator_output,
    )?;
    let fitness_evidence_path = if evaluation_value
        .get("fitness_evidence_path")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|path| !path.trim().is_empty())
    {
        "[present; path omitted from coder feedback]"
    } else {
        "none"
    };

    let feedback = format!(
        "SENIOR SWE-BENCH EVALUATOR FEEDBACK (from previous candidate patch; coder-visible context only, not a seeded fitness_report or failure_report):\n\
         - task_id: {task_id}\n\
         - repo: {repo}\n\
         - evaluator_kind: {evaluator}\n\
         - status: {status}\n\
         - exit_code: {exit_code}\n\
         - candidate_patch_hash: {candidate_patch_hash}\n\
         - fitness_evidence_path: {fitness_evidence_path}\n\
         - stdout_preview: {stdout_preview}\n\
         - stderr_preview: {stderr_preview}\n\n\
         Use this evaluator feedback to revise the next candidate patch. Preserve the no-GitHub/public-solution-search rule, solve only from supplied local checkout context and local tests, and return only a unified diff candidate patch."
    );

    let object = cycle_value
        .as_object_mut()
        .ok_or_else(|| "Senior SWE-Bench cycle input must be a JSON object".to_string())?;
    let design = object
        .get("design")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    object.insert(
        "design".to_string(),
        serde_json::Value::String(format!("{design}\n\n{feedback}")),
    );
    let plan = object
        .get("plan")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    object.insert(
        "plan".to_string(),
        serde_json::Value::String(format!(
            "1. Read the Senior SWE-Bench evaluator feedback from the previous candidate patch.\n2. Use the supplied checkout context/local tests to address the reported failure without public solution search.\n3. Return only a revised unified diff candidate patch.\n\nPrevious plan:\n{plan}"
        )),
    );
    if let Some(evaluation) = object
        .get_mut("evaluation")
        .and_then(serde_json::Value::as_object_mut)
    {
        evaluation.insert(
            "status".to_string(),
            serde_json::Value::String("not_evaluated".to_string()),
        );
        evaluation.insert("fitness".to_string(), serde_json::Value::Null);
        evaluation.insert(
            "note".to_string(),
            serde_json::Value::String(
                "previous evaluator feedback is included in design; run evaluator again before claiming task fitness".to_string(),
            ),
        );
    }
    Ok(cycle_value)
}

fn safe_feedback_atom(field: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(|ch| ch.is_control()) {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} is not safe for coder feedback"
        ));
    }
    if contains_public_solution_reference_in_feedback(trimmed) {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} contains public solution reference"
        ));
    }
    Ok(trimmed.to_string())
}

fn safe_benchmark_identifier(
    field: &str,
    value: &str,
    allow_single_slash: bool,
) -> Result<String, String> {
    let safe = safe_feedback_atom(field, value)?;
    if safe.len() > 200 {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} is too long for coder feedback"
        ));
    }
    let slash_count = safe.chars().filter(|ch| *ch == '/').count();
    if (!allow_single_slash && slash_count > 0) || (allow_single_slash && slash_count != 1) {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} is not a safe benchmark identifier"
        ));
    }
    if !safe
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'))
    {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} is not a safe benchmark identifier"
        ));
    }
    if safe.starts_with('/') || safe.ends_with('/') || safe.contains("//") {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} is not a safe benchmark identifier"
        ));
    }
    Ok(safe)
}

fn safe_evaluation_status(value: &str) -> Result<String, String> {
    match safe_feedback_atom("status", value)?.as_str() {
        "passed" => Ok("passed".to_string()),
        "failed" => Ok("failed".to_string()),
        other => Err(format!(
            "Senior SWE-Bench local evaluation status is not a reviewed value: {other}"
        )),
    }
}

fn safe_evaluator_kind(value: &str) -> Result<String, String> {
    match safe_feedback_atom("evaluator", value)?.as_str() {
        "provided_local_command" => Ok("provided_local_command".to_string()),
        "official_senior_swe_bench" => Ok("official_senior_swe_bench".to_string()),
        other => Err(format!(
            "Senior SWE-Bench local evaluation evaluator is not a reviewed value: {other}"
        )),
    }
}

fn safe_candidate_patch_hash(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() < 6 || !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            "Senior SWE-Bench local evaluation candidate_patch_hash must be a hex git hash"
                .to_string(),
        );
    }
    safe_feedback_atom("candidate_patch_hash", trimmed)
}

fn feedback_preview_for_cycle_input(
    evaluation_value: &serde_json::Value,
    field: &str,
    may_show_evaluator_output: bool,
) -> Result<String, String> {
    let Some(raw) = evaluation_value
        .get(field)
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(String::new());
    };
    if !may_show_evaluator_output {
        return Ok(
            "[redacted: evaluator output is not declared public local-test feedback]".to_string(),
        );
    }
    if contains_public_solution_reference_in_feedback(raw) {
        return Err(format!(
            "Senior SWE-Bench local evaluation {field} contains public solution reference"
        ));
    }
    Ok(preview_text(raw))
}

fn reject_reserved_cycle_feedback_artifacts(value: &serde_json::Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "Senior SWE-Bench cycle input must be a JSON object".to_string())?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "requirements" | "design" | "plan" | "benchmark_context" | "evaluation"
        ) {
            return Err(format!(
                "Senior SWE-Bench cycle input feedback cannot preserve non-task-context artifact: {key}"
            ));
        }
    }
    reject_reserved_cycle_feedback_artifacts_at(value, "$")
}

fn reject_reserved_cycle_feedback_artifacts_at(
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, nested) in object {
                let nested_path = format!("{path}.{key}");
                if matches!(
                    key.as_str(),
                    "fitness_report"
                        | "failure_report"
                        | "provider_health_report"
                        | "provider_policy"
                        | "system_code"
                        | "system_patch"
                        | "test_results"
                        | "enzyme_defs"
                        | "code"
                        | "benchmark_checkout_context"
                ) {
                    return Err(format!(
                        "Senior SWE-Bench cycle input feedback cannot preserve reserved runtime artifact: {nested_path}"
                    ));
                }
                reject_reserved_cycle_feedback_artifacts_at(nested, &nested_path)?;
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                reject_reserved_cycle_feedback_artifacts_at(nested, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn reject_public_solution_refs_in_cycle_feedback_content(
    value: &serde_json::Value,
) -> Result<(), String> {
    reject_public_solution_refs_in_cycle_feedback_content_at(value, "$")
}

fn reject_public_solution_refs_in_cycle_feedback_content_at(
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    match value {
        serde_json::Value::String(text) => {
            if contains_public_solution_reference_in_feedback(text) {
                Err(format!(
                    "Senior SWE-Bench cycle input feedback cannot preserve public solution reference at {path}"
                ))
            } else {
                Ok(())
            }
        }
        serde_json::Value::Object(object) => {
            for (key, nested) in object {
                reject_public_solution_refs_in_cycle_feedback_content_at(
                    nested,
                    &format!("{path}.{key}"),
                )?;
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                reject_public_solution_refs_in_cycle_feedback_content_at(
                    nested,
                    &format!("{path}[{index}]"),
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn contains_public_solution_reference_in_feedback(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("github.com")
        || lower.contains("githubusercontent.com")
        || lower.contains("/pull/")
        || lower.contains("/commit/")
        || lower.contains("/issues/")
        || lower.contains("refs/pull")
}

pub fn render_senior_swe_bench_task_context(
    task: &SeniorSweBenchTask,
    variant: &SeniorSweBenchVariant,
) -> String {
    let restrictions = senior_swe_bench_agent_restrictions();
    format!(
        "{preamble}\n\nTask: {task_id}\nFamily: {family}\nRepository: {repo_slug}\nSegment: {segment}\nType: {task_type}\nDifficulty: {difficulty}\nDescription:\n{description}\n\nValidation: use the benchmark-provided checkout and local tests/harness only.\nForbidden: {forbidden}",
        preamble = restrictions.required_agent_preamble,
        task_id = variant.task_id,
        family = task.family,
        repo_slug = repository_id(task),
        segment = task.segment,
        task_type = task.task_type,
        difficulty = variant.difficulty,
        description = task.description,
        forbidden = restrictions.forbidden_actions.join("; "),
    )
}

fn decode_next_rsc_escaped_json(input: &str) -> String {
    let mut decoded = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('\\') => {
                    chars.next();
                    decoded.push('\\');
                }
                Some('"') => {
                    chars.next();
                    decoded.push('"');
                }
                _ => decoded.push(ch),
            }
        } else {
            decoded.push(ch);
        }
    }
    decoded
}

fn find_json_array_end(input: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in input[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(start + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }

    None
}

fn required_string(value: &serde_json::Value, field: &str) -> Result<String, String> {
    let lookup = field.rsplit('.').next().unwrap_or(field);
    value
        .get(lookup)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Senior SWE-Bench artifact missing {field}"))
}

fn ensure_solution_search_forbidden(allowed: bool, source: &str) -> Result<(), String> {
    if allowed {
        Err(format!(
            "Senior SWE-Bench {source} allows GitHub solution search; refusing evaluation"
        ))
    } else {
        Ok(())
    }
}

fn preview_text(value: &str) -> String {
    const LIMIT: usize = 2000;
    let mut preview = value.chars().take(LIMIT).collect::<String>();
    if value.chars().count() > LIMIT {
        preview.push_str("...[truncated]");
    }
    preview
}

fn bump(map: &mut BTreeMap<String, usize>, key: String) {
    *map.entry(key).or_insert(0) += 1;
}

fn repository_id(task: &SeniorSweBenchTask) -> String {
    if !task.repo_slug.trim().is_empty() {
        task.repo_slug.clone()
    } else {
        non_empty(&task.repo, "unknown")
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_next_payload() -> &'static str {
        r#"self.__next_f.push([1,"{\"taxonomy_options\":[],\"tasks\":[{\"family\":\"firezone-fix-connlib-align-device\",\"repo\":\"firezone\",\"repo_slug\":\"firezone/firezone\",\"task_type\":\"bug\",\"segment\":\"investigate\",\"tags\":[\"firezone\",\"rust\"],\"in_benchmark\":true,\"in_sample\":false,\"version\":\"2026.06\",\"description\":\"Device pool resources fail silently.\\nFix from local evidence only.\",\"taxonomy\":{\"task_type\":\"bug\",\"stack\":[\"rust\"],\"skills\":[\"codebase-exploration\",\"test-execution\"],\"skill_breadth\":\"moderate\",\"runtime_dependence\":\"runtime-helpful\",\"misdirection\":\"passive\",\"estimated_human_time\":\"standard\",\"oracle_scope\":\"spread\"},\"environment\":{\"cpus\":4,\"memory\":\"8G\",\"timeout_sec\":7200},\"hard\":{\"task_id\":\"firezone-fix-connlib-align-device-hard\",\"difficulty\":\"frontier\"},\"guided\":{\"task_id\":\"firezone-fix-connlib-align-device-guided\",\"difficulty\":\"challenging\"}}],\"other\":true}"]);"#
    }

    #[test]
    fn extracts_tasks_from_next_rsc_payload_without_github_search() {
        let tasks = extract_senior_swe_bench_tasks(sample_next_payload()).unwrap();
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.family, "firezone-fix-connlib-align-device");
        assert_eq!(task.repo_slug, "firezone/firezone");
        assert_eq!(task.taxonomy.stack, vec!["rust"]);
        assert_eq!(
            task.hard.as_ref().unwrap().task_id,
            "firezone-fix-connlib-align-device-hard"
        );
    }

    #[test]
    fn catalog_audit_records_policy_counts_and_hidden_holdout_scope() {
        let tasks = extract_senior_swe_bench_tasks(sample_next_payload()).unwrap();
        let audit = build_senior_swe_bench_audit(&tasks, "fixture");
        assert_eq!(audit.schema_version, SENIOR_SWE_BENCH_AUDIT_SCHEMA);
        assert_eq!(audit.task_count, 1);
        assert_eq!(audit.benchmark_task_count, 1);
        assert_eq!(audit.repo_count, 1);
        assert_eq!(audit.repos, vec!["firezone/firezone"]);
        assert_eq!(audit.stacks.get("rust"), Some(&1));
        assert_eq!(audit.difficulties.get("hard:frontier"), Some(&1));
        assert!(!audit.agent_restrictions.github_solution_search_allowed);
        assert!(
            audit
                .hidden_holdout_applicability
                .contains("catalog-audit-only")
        );
    }

    #[test]
    fn rendered_task_context_forbids_solution_search() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let context = render_senior_swe_bench_task_context(&task, task.hard.as_ref().unwrap());
        assert!(context.contains("Do not search GitHub"));
        assert!(context.contains("firezone-fix-connlib-align-device-hard"));
        assert!(context.contains("benchmark-provided checkout and local tests/harness only"));
    }

    #[test]
    fn task_package_carries_context_policy_and_not_evaluated_status() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let package =
            build_senior_swe_bench_task_package(&task, "hard", task.hard.as_ref().unwrap());

        assert_eq!(package.schema_version, SENIOR_SWE_BENCH_TASK_PACKAGE_SCHEMA);
        assert_eq!(package.task_id, "firezone-fix-connlib-align-device-hard");
        assert_eq!(package.repo, "firezone/firezone");
        assert_eq!(package.variant, "hard");
        assert!(package.in_benchmark);
        assert!(!package.in_sample);
        assert_eq!(package.tags, vec!["firezone", "rust"]);
        assert!(!package.agent_restrictions.github_solution_search_allowed);
        assert!(
            package
                .coding_agent_context
                .contains("Do not search GitHub")
        );
        assert_eq!(package.evaluation.status, "not_evaluated");
        assert_eq!(package.evaluation.evaluator, "official_senior_swe_bench");
        assert_eq!(package.evaluation.fitness, None);
    }

    #[test]
    fn cycle_input_carries_patch_deliverable_no_solution_search_and_not_evaluated_status() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());

        assert!(
            input["requirements"]
                .as_str()
                .unwrap()
                .contains("unified diff candidate patch")
        );
        assert!(
            input["requirements"]
                .as_str()
                .unwrap()
                .contains("Do not search GitHub")
        );
        assert!(input["design"].as_str().unwrap().contains("local checkout"));
        assert!(
            input["plan"]
                .as_str()
                .unwrap()
                .contains("Return only a unified diff")
        );
        assert_eq!(
            input["benchmark_context"]["task_id"].as_str(),
            Some("firezone-fix-connlib-align-device-hard")
        );
        assert_eq!(
            input["benchmark_context"]["github_solution_search_allowed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            input["evaluation"]["status"].as_str(),
            Some("not_evaluated")
        );
        assert!(input["evaluation"]["fitness"].is_null());
    }

    #[test]
    fn cycle_input_parser_accepts_only_unevaluated_no_search_inputs() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());

        let summary = parse_senior_swe_bench_cycle_input(&input.to_string()).unwrap();
        assert_eq!(summary.task_id, "firezone-fix-connlib-align-device-hard");
        assert_eq!(summary.repo, "firezone/firezone");
        assert!(!summary.github_solution_search_allowed);

        let mut allows_search = input.clone();
        allows_search["benchmark_context"]["github_solution_search_allowed"] =
            serde_json::json!(true);
        assert!(
            parse_senior_swe_bench_cycle_input(&allows_search.to_string())
                .unwrap_err()
                .contains("allows GitHub solution search")
        );

        let mut already_evaluated = input.clone();
        already_evaluated["evaluation"]["status"] = serde_json::json!("passed");
        assert!(
            parse_senior_swe_bench_cycle_input(&already_evaluated.to_string())
                .unwrap_err()
                .contains("not_evaluated")
        );

        let mut fitness_bearing = input;
        fitness_bearing["evaluation"]["fitness"] = serde_json::json!(1.0);
        assert!(
            parse_senior_swe_bench_cycle_input(&fitness_bearing.to_string())
                .unwrap_err()
                .contains("fitness must be null")
        );
    }

    #[test]
    fn cycle_input_feedback_injects_evaluator_failure_without_reserved_artifacts() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch": "candidate.diff",
            "candidate_patch_hash": "abc12300",
            "checkout": "checkout",
            "evaluator_checkout": "checkout",
            "candidate_patch_applied": true,
            "evaluator_checkout_mode": "isolated_copy",
            "original_checkout_mutated": false,
            "candidate_patch_preflight_checked": true,
            "candidate_patch_preflight_status": "passed",
            "candidate_patch_preflight_command": "git apply --check -- candidate.diff",
            "source_revision": "rev",
            "source_tree_dirty": true,
            "source_diff_scope": "crates",
            "source_diff_hash": "hash",
            "evidence_command": "senior-swe-bench-evaluate ...",
            "evaluator_command": ["./evaluator"],
            "github_solution_search_allowed": false,
            "feedback_visibility": "public_local_test_output",
            "stdout_preview": "public local test output",
            "stderr_preview": "missing public local route assertion",
            "fitness_evidence_path": null,
            "note": "local evaluator wrapper only"
        });

        let feedback = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .unwrap();

        assert_eq!(
            feedback["evaluation"]["status"].as_str(),
            Some("not_evaluated")
        );
        assert!(feedback["evaluation"]["fitness"].is_null());
        assert!(feedback.get("fitness_report").is_none());
        assert!(feedback.get("failure_report").is_none());
        let design = feedback["design"].as_str().unwrap();
        assert!(design.contains("SENIOR SWE-BENCH EVALUATOR FEEDBACK"));
        assert!(design.contains("status: failed"));
        assert!(design.contains("candidate_patch_hash: abc12300"));
        assert!(design.contains("missing public local route assertion"));
        assert!(design.contains("not a seeded fitness_report or failure_report"));
        assert!(design.contains("no-GitHub/public-solution-search"));
        assert_eq!(
            feedback["benchmark_context"]["github_solution_search_allowed"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn cycle_input_feedback_redacts_official_hidden_holdout_output() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "official_senior_swe_bench",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "official_hidden_holdouts": true,
            "stdout_preview": "SECRET_PUBLIC_TEST_CONTEXT",
            "stderr_preview": "SECRET_HIDDEN_HOLDOUT_FAILURE"
        });

        let feedback = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .unwrap();
        let design = feedback["design"].as_str().unwrap();

        assert!(design.contains("not declared public local-test feedback"));
        assert!(!design.contains("SECRET_PUBLIC_TEST_CONTEXT"));
        assert!(!design.contains("SECRET_HIDDEN_HOLDOUT_FAILURE"));
    }

    #[test]
    fn cycle_input_feedback_redacts_local_output_unless_visibility_is_public() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "stdout_preview": "POTENTIALLY_HIDDEN_OUTPUT",
            "stderr_preview": "POTENTIALLY_HIDDEN_FAILURE"
        });

        let feedback = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .unwrap();
        let design = feedback["design"].as_str().unwrap();

        assert!(design.contains("not declared public local-test feedback"));
        assert!(!design.contains("POTENTIALLY_HIDDEN_OUTPUT"));
        assert!(!design.contains("POTENTIALLY_HIDDEN_FAILURE"));
    }

    #[test]
    fn cycle_input_feedback_rejects_public_solution_references_in_visible_feedback() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "feedback_visibility": "public_local_test_output",
            "stdout_preview": "see https://github.com/firezone/firezone/pull/123",
            "stderr_preview": ""
        });

        let err = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .expect_err("public solution references must not enter coder feedback");

        assert!(err.contains("public solution reference"), "{err}");
    }

    #[test]
    fn cycle_input_feedback_rejects_solution_references_already_in_cycle_input() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let mut input =
            build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        input["design"] = serde_json::json!(
            "previous attempt copied https://github.com/firezone/firezone/pull/123"
        );
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false
        });

        let err = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .expect_err("cycle input solution references must not be preserved");

        assert!(err.contains("public solution reference"), "{err}");
        assert!(err.contains("$.design"), "{err}");
    }

    #[test]
    fn cycle_input_feedback_rejects_solution_references_in_feedback_metadata() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "https://github.com/firezone/firezone/commit/deadbeef",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "fitness_evidence_path": "https://github.com/firezone/firezone/pull/123"
        });

        let err = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .expect_err("metadata fields must not carry public solution references");

        assert!(
            err.contains("status contains public solution reference"),
            "{err}"
        );
    }

    #[test]
    fn cycle_input_feedback_omits_fitness_evidence_path_from_design() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "fitness_evidence_path": "runs/private/hidden/failure.json"
        });

        let feedback = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .unwrap();
        let design = feedback["design"].as_str().unwrap();

        assert!(design.contains("path omitted from coder feedback"));
        assert!(!design.contains("runs/private/hidden/failure.json"));
    }

    #[test]
    fn cycle_input_feedback_rejects_reserved_runtime_artifacts() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let mut input =
            build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        input["benchmark_context"]["metadata"] = serde_json::json!({
            "nested": [{"fitness_report": {"fake": true}}]
        });
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false
        });

        let err = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .expect_err("reserved runtime artifacts must not be preserved by feedback command");

        assert!(err.contains("reserved runtime artifact"), "{err}");
        assert!(err.contains("fitness_report"), "{err}");
    }

    #[test]
    fn cycle_input_feedback_rejects_malformed_official_hidden_holdouts() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "firezone-fix-connlib-align-device-hard",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "exit_code": 2,
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false,
            "official_hidden_holdouts": "true",
            "stdout_preview": "SECRET"
        });

        let err = build_senior_swe_bench_cycle_input_feedback(
            &input.to_string(),
            &local_evaluation.to_string(),
        )
        .expect_err("malformed hidden-holdout provenance must fail closed");

        assert!(
            err.contains("official_hidden_holdouts must be boolean"),
            "{err}"
        );
    }

    #[test]
    fn cycle_input_feedback_rejects_mismatched_or_solution_search_evaluation() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let input = build_senior_swe_bench_cycle_input(&task, "hard", task.hard.as_ref().unwrap());
        let mut local_evaluation = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA,
            "task_id": "other-task",
            "repo": "firezone/firezone",
            "evaluator": "provided_local_command",
            "status": "failed",
            "candidate_patch_hash": "abc12300",
            "github_solution_search_allowed": false
        });

        assert!(
            build_senior_swe_bench_cycle_input_feedback(
                &input.to_string(),
                &local_evaluation.to_string(),
            )
            .unwrap_err()
            .contains("does not match cycle input")
        );

        local_evaluation["task_id"] = serde_json::json!("firezone-fix-connlib-align-device-hard");
        local_evaluation["github_solution_search_allowed"] = serde_json::json!(true);
        assert!(
            build_senior_swe_bench_cycle_input_feedback(
                &input.to_string(),
                &local_evaluation.to_string(),
            )
            .unwrap_err()
            .contains("allows GitHub solution search")
        );
    }

    #[test]
    fn task_package_parser_rejects_solution_search_allowed() {
        let task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        let package =
            build_senior_swe_bench_task_package(&task, "hard", task.hard.as_ref().unwrap());
        let mut value = serde_json::to_value(&package).unwrap();
        let summary = parse_senior_swe_bench_task_package(&value.to_string()).unwrap();
        assert_eq!(summary.task_id, "firezone-fix-connlib-align-device-hard");
        assert_eq!(summary.repo, "firezone/firezone");
        assert!(!summary.github_solution_search_allowed);

        value["agent_restrictions"]["github_solution_search_allowed"] = serde_json::json!(true);
        let error = parse_senior_swe_bench_task_package(&value.to_string()).unwrap_err();
        assert!(error.contains("allows GitHub solution search"));
    }

    #[test]
    fn official_evaluator_manifest_requires_holdouts_no_search_and_matching_command() {
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let command = vec!["./official-evaluator".to_string(), "--task".to_string()];
        let manifest = serde_json::json!({
            "schema_version": SENIOR_SWE_BENCH_OFFICIAL_EVALUATOR_MANIFEST_SCHEMA,
            "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "hidden_holdouts": true,
            "github_solution_search_allowed": false,
            "benchmark_provided_command": command,
        });
        let parsed = parse_senior_swe_bench_official_evaluator_manifest(
            &manifest.to_string(),
            &package,
            &["./official-evaluator".to_string(), "--task".to_string()],
        )
        .unwrap();
        assert_eq!(parsed.task_id, "task-hard");
        assert_eq!(parsed.repo, "owner/repo");
        assert!(parsed.hidden_holdouts);
        assert!(!parsed.github_solution_search_allowed);

        let mut no_holdouts = manifest.clone();
        no_holdouts["hidden_holdouts"] = serde_json::json!(false);
        assert!(
            parse_senior_swe_bench_official_evaluator_manifest(
                &no_holdouts.to_string(),
                &package,
                &parsed.benchmark_provided_command,
            )
            .unwrap_err()
            .contains("hidden_holdouts")
        );

        let mut allows_search = manifest.clone();
        allows_search["github_solution_search_allowed"] = serde_json::json!(true);
        assert!(
            parse_senior_swe_bench_official_evaluator_manifest(
                &allows_search.to_string(),
                &package,
                &parsed.benchmark_provided_command,
            )
            .unwrap_err()
            .contains("allows GitHub solution search")
        );

        let mut wrong_task = manifest.clone();
        wrong_task["task_id"] = serde_json::json!("other-task");
        assert!(
            parse_senior_swe_bench_official_evaluator_manifest(
                &wrong_task.to_string(),
                &package,
                &parsed.benchmark_provided_command,
            )
            .unwrap_err()
            .contains("does not match task input")
        );

        assert!(
            parse_senior_swe_bench_official_evaluator_manifest(
                &manifest.to_string(),
                &package,
                &["./different".to_string()],
            )
            .unwrap_err()
            .contains("does not match invoked evaluator command")
        );
    }

    #[test]
    fn local_evaluation_result_redacts_long_output_and_stays_not_official_claim() {
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let evaluation = build_senior_swe_bench_local_evaluation(
            &package,
            "provided_local_command",
            "passed",
            Some(0),
            "candidate.diff",
            "patchhash",
            "checkout",
            "patched-checkout",
            true,
            "isolated_copy",
            false,
            true,
            "passed",
            "git apply --check --whitespace=nowarn -- candidate.diff",
            "revision",
            true,
            "crates",
            "0123456789abcdef0123456789abcdef01234567",
            "senior-swe-bench-evaluate --task-package task.json",
            vec!["./test.sh".to_string()],
            None,
            None,
            None,
            &"x".repeat(2500),
            "",
            Some("evidence.json".to_string()),
        );
        assert_eq!(
            evaluation.schema_version,
            SENIOR_SWE_BENCH_LOCAL_EVALUATION_SCHEMA
        );
        assert_eq!(evaluation.status, "passed");
        assert_eq!(evaluation.candidate_patch_hash, "patchhash");
        assert_eq!(evaluation.evaluator_checkout, "patched-checkout");
        assert!(evaluation.candidate_patch_applied);
        assert_eq!(evaluation.evaluator_checkout_mode, "isolated_copy");
        assert!(!evaluation.original_checkout_mutated);
        assert!(evaluation.candidate_patch_preflight_checked);
        assert_eq!(evaluation.candidate_patch_preflight_status, "passed");
        assert!(
            evaluation
                .candidate_patch_preflight_command
                .contains("git apply --check")
        );
        assert_eq!(evaluation.source_revision, "revision");
        assert!(evaluation.source_tree_dirty);
        assert_eq!(evaluation.source_diff_scope, "crates");
        assert_eq!(
            evaluation.source_diff_hash,
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert!(
            evaluation
                .evidence_command
                .contains("senior-swe-bench-evaluate")
        );
        assert!(evaluation.stdout_preview.ends_with("...[truncated]"));
        assert!(evaluation.note.contains("local evaluator wrapper only"));
        assert!(!evaluation.github_solution_search_allowed);
    }

    #[test]
    fn audit_falls_back_to_repo_when_repo_slug_is_absent() {
        let mut task = extract_senior_swe_bench_tasks(sample_next_payload())
            .unwrap()
            .remove(0);
        task.repo_slug.clear();
        task.repo = "firezone".to_string();
        let audit = build_senior_swe_bench_audit(&[task], "fixture");
        assert_eq!(audit.repos, vec!["firezone"]);
    }

    #[test]
    fn extracts_tasks_with_embedded_escaped_quotes_from_live_shape() {
        let payload = r#"self.__next_f.push([1,"{\"tasks\":[{\"family\":\"gitea-add-project-column-picker\",\"repo_slug\":\"go-gitea/gitea\",\"task_type\":\"feature\",\"segment\":\"design\",\"description\":\"Move cards from columns like \\\"To Do\\\" / \\\"In Progress\\\" without opening the board.\",\"in_benchmark\":true,\"in_sample\":true,\"taxonomy\":{\"stack\":[\"go\",\"html-css\"]},\"hard\":{\"task_id\":\"gitea-add-project-column-picker-hard\",\"difficulty\":\"challenging\"}}]}"]);"#;
        let tasks = extract_senior_swe_bench_tasks(payload).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].family, "gitea-add-project-column-picker");
        assert!(tasks[0].description.contains("\"To Do\""));
        assert_eq!(tasks[0].taxonomy.stack, vec!["go", "html-css"]);
    }
}
