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

        if !context.relevant_files.is_empty() {
            prompt.push_str("## Relevant Files\n\n");
            for f in &context.relevant_files {
                prompt.push_str(&format!("- {}\n", f.display()));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Instructions\n\n");
        prompt.push_str("Produce a unified diff that addresses the task.\n");
        prompt.push_str("Explain your rationale.\n");
        prompt.push_str("Mention each acceptance criterion in your rationale.\n");

        prompt
    }

    fn build_system_prompt(&self) -> &'static str {
        "You are a software engineer working on the A² project. \
         Produce clean, minimal patches that address the task. \
         Output a unified diff followed by a rationale section. \
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
            let rationale = format!(
                "{}{}",
                &text[..start].trim(),
                &text[after_fence + end + 3..].trim()
            );
            return (diff, rationale);
        }
    }
    // No diff block found — whole response is rationale, empty diff.
    (String::new(), text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_diff_from_rationale() {
        let text = "Here's the fix:\n\n```diff\n--- a/foo.rs\n+++ b/foo.rs\n+fixed\n```\n\nThis works because reasons.";
        let (diff, rationale) = split_response(text);
        assert!(diff.contains("+fixed"));
        assert!(rationale.contains("reasons"));
        assert!(!rationale.contains("```"));
    }

    #[test]
    fn no_diff_block_returns_empty_diff() {
        let text = "I couldn't produce a diff but here's my analysis.";
        let (diff, rationale) = split_response(text);
        assert!(diff.is_empty());
        assert!(rationale.contains("analysis"));
    }
}
