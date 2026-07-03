//! Senior SWE-Bench catalog parsing and task-context policy helpers.
//!
//! The benchmark task listing is currently published as a Next.js/RSC page.
//! A²D only needs the public task metadata at this layer; coding agents must
//! receive sanitized task context and an explicit no-solution-search policy.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SENIOR_SWE_BENCH_AUDIT_SCHEMA: &str = "a2d.senior-swe-bench-audit.v1";

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
