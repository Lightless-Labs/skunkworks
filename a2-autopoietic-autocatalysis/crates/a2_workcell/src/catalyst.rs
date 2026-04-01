//! Generalist catalyst — the Stage 0 "do anything" catalyst.
//!
//! Takes a TaskContract, builds a prompt, calls a model, and parses
//! the response into a PatchBundle. This is the seed catalyst that
//! bootstraps A² — it will be differentiated into specialists later.

use a2_core::error::A2Result;
use a2_core::id::*;
use a2_core::protocol::*;
use a2_core::traits::*;
use chrono::Utc;

pub struct GeneralistCatalyst {
    id: CatalystId,
}

impl GeneralistCatalyst {
    pub fn new() -> Self {
        Self {
            id: CatalystId::new(),
        }
    }

    fn workspace_structure(&self) -> &'static str {
        "- `Cargo.toml`: workspace root\n\
         - `DESIGN.md`: architectural reference\n\
         - `crates/a2d`: control plane / governor\n\
         - `crates/a2ctl`: CLI entrypoint\n\
         - `crates/a2_workcell`: workcell runtime and catalyst prompt logic\n\
         - `crates/a2_membrane`: policy enforcement\n\
         - `crates/a2_broker`: model/provider routing\n\
         - `crates/a2_constitution`: constitutional kernel and verifiers\n\
         - `crates/a2_eval`: evaluators and sentinels\n\
         - `crates/a2_archive`: lineage and archive storage\n\
         - `crates/a2_raf`: causal graph and RAF diagnostics\n\
         - `crates/a2_sensorium`: evidence/task ingestion\n\
         - `constitution/`, `policies/`, `schemas/`, `prompts/`, `docs/`, `bench/`, `lineage/`: supporting inputs and artifacts\n"
    }

    fn build_prompt(&self, task: &TaskContract, context: &ContextPack) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!("# Task: {}\n\n", task.title));
        prompt.push_str(&format!("{}\n\n", task.description));

        if !task.acceptance_criteria.is_empty() {
            prompt.push_str("## Acceptance Criteria\n\n");
            for criterion in &task.acceptance_criteria {
                prompt.push_str(&format!("- {criterion}\n"));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Workspace Structure\n\n");
        prompt.push_str(self.workspace_structure());
        prompt.push('\n');

        if !context.relevant_files.is_empty() {
            prompt.push_str("## Relevant Files\n\n");
            for f in &context.relevant_files {
                prompt.push_str(&format!("- {}\n", f.display()));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Instructions\n\n");
        prompt.push_str("Produce a clean, minimal unified diff that addresses the task.\n");
        prompt.push_str("Use explicit `Diff`, `Rationale`, and `Test Suggestions` sections.\n");
        prompt.push_str("Mention each acceptance criterion explicitly in the rationale.\n");
        prompt.push_str("Suggest the most relevant tests or validation commands, even if you did not run them.\n\n");

        prompt.push_str("## Output Format\n\n");
        prompt.push_str("Respond in exactly this structure:\n\n");
        prompt.push_str("## Diff\n");
        prompt.push_str("```diff\n");
        prompt.push_str("--- a/path\n");
        prompt.push_str("+++ b/path\n");
        prompt.push_str("@@\n");
        prompt.push_str("```\n\n");
        prompt.push_str("## Rationale\n");
        prompt.push_str("- Briefly explain the change.\n");
        prompt.push_str("- Mention each acceptance criterion and how the patch satisfies it.\n\n");
        prompt.push_str("## Test Suggestions\n");
        prompt.push_str("- List focused tests, commands, or manual checks.\n");

        prompt
    }

    fn build_system_prompt(&self) -> &'static str {
        "You are a software engineer working on the A² project. \
         Produce clean, minimal patches that address the task. \
         Output explicit Diff, Rationale, and Test Suggestions sections. \
         Be concise."
    }
}

impl Default for GeneralistCatalyst {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Catalyst for GeneralistCatalyst {
    fn id(&self) -> &CatalystId {
        &self.id
    }

    fn name(&self) -> &str {
        "generalist"
    }

    async fn execute(
        &self,
        task: &TaskContract,
        context: &ContextPack,
        model: &dyn ModelProvider,
    ) -> A2Result<PatchBundle> {
        let prompt = self.build_prompt(task, context);
        let system = self.build_system_prompt();

        let response = model.generate(&prompt, Some(system)).await?;

        // Parse: treat the whole response as the diff + rationale for now.
        // Stage 0 is intentionally simple — structured output parsing comes later.
        let (diff, rationale) = split_response(&response.text);

        Ok(PatchBundle {
            id: PatchId::new(),
            task_id: task.id.clone(),
            workcell_id: WorkcellId::new(),
            diff,
            rationale,
            test_results: TestResults {
                passed: 0,
                failed: 0,
                skipped: 0,
                details: vec![],
            },
            model_attribution: ModelAttribution {
                provider: model.provider_id().into(),
                model: model.model_id().into(),
                tokens_in: response.tokens_in,
                tokens_out: response.tokens_out,
            },
            created_at: Utc::now(),
        })
    }
}

/// Split model response into diff and rationale sections.
/// Looks for a ```diff block; if not found, treats everything as rationale.
fn split_response(text: &str) -> (String, String) {
    if let Some(start) = text.find("```diff") {
        let after_fence = start + 7;
        if let Some(end) = text[after_fence..].find("```") {
            let diff = text[after_fence..after_fence + end].trim().to_string();
            let before_diff = strip_trailing_heading(&text[..start], &["diff"]);
            let after_diff = strip_leading_heading(&text[after_fence + end + 3..], &["rationale"]);
            let rationale = join_non_empty_sections([before_diff, after_diff]);
            return (diff, rationale);
        }
    }
    // No diff block found — whole response is rationale, empty diff.
    (String::new(), text.to_string())
}

fn strip_trailing_heading(text: &str, headings: &[&str]) -> String {
    let mut lines: Vec<&str> = text.trim().lines().collect();
    if matches!(lines.last(), Some(line) if is_heading(line, headings)) {
        lines.pop();
    }
    lines.join("\n").trim().to_string()
}

fn strip_leading_heading(text: &str, headings: &[&str]) -> String {
    let mut lines = text.trim().lines();
    if matches!(lines.clone().next(), Some(line) if is_heading(line, headings)) {
        lines.next();
    }
    lines.collect::<Vec<_>>().join("\n").trim().to_string()
}

fn is_heading(line: &str, headings: &[&str]) -> bool {
    let normalized = line
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_end_matches(':')
        .trim()
        .to_ascii_lowercase();

    headings.iter().any(|heading| normalized == *heading)
}

fn join_non_empty_sections<const N: usize>(sections: [String; N]) -> String {
    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_task() -> TaskContract {
        TaskContract {
            id: TaskId::new(),
            title: "Improve catalyst prompt".into(),
            description: "Tighten the prompt template and output contract.".into(),
            acceptance_criteria: vec!["Includes workspace context".into()],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            created_at: Utc::now(),
        }
    }

    fn make_context() -> ContextPack {
        ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![PathBuf::from("crates/a2_workcell/src/catalyst.rs")],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        }
    }

    #[test]
    fn prompt_includes_workspace_structure_and_output_contract() {
        let catalyst = GeneralistCatalyst::new();
        let prompt = catalyst.build_prompt(&make_task(), &make_context());

        assert!(prompt.contains("## Workspace Structure"));
        assert!(prompt.contains("crates/a2_workcell"));
        assert!(prompt.contains("## Output Format"));
        assert!(prompt.contains("## Diff"));
        assert!(prompt.contains("## Rationale"));
        assert!(prompt.contains("## Test Suggestions"));
        assert!(prompt.contains("Suggest the most relevant tests"));
    }

    #[test]
    fn splits_diff_from_rationale() {
        let text = "Here's the fix:\n\n```diff\n--- a/foo.rs\n+++ b/foo.rs\n+fixed\n```\n\nThis works because reasons.";
        let (diff, rationale) = split_response(text);
        assert!(diff.contains("+fixed"));
        assert!(rationale.contains("reasons"));
        assert!(!rationale.contains("```"));
    }

    #[test]
    fn strips_structured_diff_and_rationale_headings() {
        let text = "## Diff\n\n```diff\n--- a/foo.rs\n+++ b/foo.rs\n+fixed\n```\n\n## Rationale\nImproves the prompt structure.\n\n## Test Suggestions\n- cargo test -p a2_workcell";
        let (diff, rationale) = split_response(text);

        assert!(diff.contains("+fixed"));
        assert!(!rationale.contains("## Diff"));
        assert!(!rationale.starts_with("## Rationale"));
        assert!(rationale.contains("Improves the prompt structure."));
        assert!(rationale.contains("## Test Suggestions"));
    }

    #[test]
    fn no_diff_block_returns_empty_diff() {
        let text = "I couldn't produce a diff but here's my analysis.";
        let (diff, rationale) = split_response(text);
        assert!(diff.is_empty());
        assert!(rationale.contains("analysis"));
    }
}
