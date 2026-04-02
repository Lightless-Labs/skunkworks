//! Worktree catalyst — runs a model CLI agent directly in a git worktree.
//!
//! Instead of asking a model to produce a unified diff as text (which fails
//! because models can't count whitespace), this catalyst:
//!
//! 1. Creates a git worktree from the current germline
//! 2. Points a full coding agent (claude/codex/gemini) at it with --full-auto
//! 3. Lets the agent edit files directly
//! 4. Captures the diff from `git diff` in the worktree
//! 5. Cleans up the worktree
//!
//! The model does what it's good at (editing code). Git does what it's good
//! at (computing diffs). The diff quality problem disappears.

use a2_core::error::{A2Error, A2Result};
use a2_core::id::*;
use a2_core::protocol::*;
use a2_core::traits::*;
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// A catalyst that runs model agents in isolated git worktrees.
pub struct WorktreeCatalyst {
    id: CatalystId,
    /// Root of the main workspace (where .git lives).
    workspace_root: PathBuf,
}

impl WorktreeCatalyst {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            id: CatalystId::new(),
            workspace_root,
        }
    }

    /// Create a temporary git worktree and return its path.
    async fn create_worktree(&self) -> A2Result<PathBuf> {
        let branch_name = format!("a2-wc-{}", uuid::Uuid::now_v7());
        let worktree_path = std::env::temp_dir().join(&branch_name);

        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch_name])
            .arg(&worktree_path)
            .arg("HEAD")
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| A2Error::CatalystFailure(self.id.clone(), format!("git worktree add: {e}")))?;

        if !output.status.success() {
            return Err(A2Error::CatalystFailure(
                self.id.clone(),
                format!(
                    "git worktree add failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(worktree_path)
    }

    /// Remove a git worktree and its branch.
    async fn cleanup_worktree(&self, worktree_path: &Path, branch_name: &str) {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(worktree_path)
            .current_dir(&self.workspace_root)
            .output()
            .await;

        let _ = Command::new("git")
            .args(["branch", "-D", branch_name])
            .current_dir(&self.workspace_root)
            .output()
            .await;
    }

    /// Capture `git diff` from the worktree (uncommitted changes).
    async fn capture_diff(&self, worktree_path: &Path) -> A2Result<String> {
        let output = Command::new("git")
            .args(["diff", "--no-color"])
            .current_dir(worktree_path)
            .output()
            .await
            .map_err(|e| A2Error::CatalystFailure(self.id.clone(), format!("git diff: {e}")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Run a model CLI agent in the worktree directory.
    /// Returns (stdout_text, tokens_in, tokens_out).
    async fn run_agent(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, u64, u64)> {
        match provider_id {
            "claude" => self.run_claude(model_id, prompt, worktree_path).await,
            "codex" => self.run_codex(model_id, prompt, worktree_path).await,
            "gemini" => self.run_gemini(model_id, prompt, worktree_path).await,
            other => Err(A2Error::CatalystFailure(
                self.id.clone(),
                format!("worktree catalyst doesn't support provider: {other}"),
            )),
        }
    }

    async fn run_claude(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, u64, u64)> {
        let output = Command::new("claude")
            .args([
                "-p", prompt,
                "--model", model_id,
                "--output-format", "stream-json",
                "--verbose",
                "--max-turns", "30",
                "--dangerously-skip-permissions",
            ])
            .current_dir(worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("claude: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut text = String::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;

        for line in stdout.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                // Collect text
                if let Some(delta) = v.get("delta").and_then(|d| d.get("text")).and_then(|t| t.as_str()) {
                    text.push_str(delta);
                }
                // Collect usage
                if v.get("type").and_then(|t| t.as_str()) == Some("result")
                    && let Some(usage) = v.get("usage")
                {
                    tokens_in = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    tokens_out = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                }
            }
        }

        Ok((text, tokens_in, tokens_out))
    }

    async fn run_codex(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, u64, u64)> {
        let output = Command::new("codex")
            .args([
                "exec", prompt,
                "-m", model_id,
                "-c", "model_reasoning_effort=\"high\"",
                "--full-auto",
            ])
            .arg("--json")
            .current_dir(worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("codex: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut text = String::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;

        for line in stdout.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(msg) = v.get("item").and_then(|i| i.get("text")).and_then(|t| t.as_str()) {
                    text.push_str(msg);
                }
                if v.get("type").and_then(|t| t.as_str()) == Some("turn.completed")
                    && let Some(usage) = v.get("usage")
                {
                    tokens_in = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    tokens_out = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                }
            }
        }

        Ok((text, tokens_in, tokens_out))
    }

    async fn run_gemini(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, u64, u64)> {
        let output = Command::new("gemini")
            .args([
                "-p", prompt,
                "--model", model_id,
                "-s", "false",
                "-y",
                "-o", "text",
            ])
            .current_dir(worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("gemini: {e}")))?;

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        // Gemini text mode doesn't expose token counts
        Ok((text, 0, 0))
    }

    fn build_prompt(&self, task: &TaskContract) -> String {
        format!(
            "You are working on the A² project.\n\n\
             # Task: {}\n\n\
             {}\n\n\
             ## Instructions\n\n\
             Read the relevant files, make the necessary changes directly, and run \
             `cargo check` to verify your changes compile. If tests are relevant, \
             run `cargo test`. Do not produce a diff — edit the files directly.\n\n\
             Keep changes minimal and focused on the task.",
            task.title, task.description
        )
    }
}

#[async_trait::async_trait]
impl Catalyst for WorktreeCatalyst {
    fn id(&self) -> &CatalystId {
        &self.id
    }

    fn name(&self) -> &str {
        "worktree"
    }

    async fn execute(
        &self,
        task: &TaskContract,
        _context: &ContextPack,
        model: &dyn ModelProvider,
    ) -> A2Result<PatchBundle> {
        let worktree_path = self.create_worktree().await?;
        let branch_name = worktree_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let prompt = self.build_prompt(task);

        // Run the agent in the worktree
        let result = self
            .run_agent(model.provider_id(), model.model_id(), &prompt, &worktree_path)
            .await;

        let (rationale, tokens_in, tokens_out) = match result {
            Ok(r) => r,
            Err(e) => {
                self.cleanup_worktree(&worktree_path, &branch_name).await;
                return Err(e);
            }
        };

        // Capture what the agent actually changed
        let diff = self.capture_diff(&worktree_path).await?;

        // Clean up
        self.cleanup_worktree(&worktree_path, &branch_name).await;

        if diff.trim().is_empty() {
            return Err(A2Error::CatalystFailure(
                self.id.clone(),
                "agent made no changes to the worktree".into(),
            ));
        }

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
                tokens_in,
                tokens_out,
            },
            created_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Bootstrap a minimal git repo with one commit so `git worktree add HEAD` works.
    async fn init_git_repo(dir: &Path) {
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@a2"],
            vec!["config", "user.name", "A2 Test"],
        ] {
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .output()
                .await
                .unwrap();
        }
        fs::write(dir.join("README.md"), "# repo\n").unwrap();
        for args in [vec!["add", "."], vec!["commit", "-m", "initial"]] {
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .output()
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn worktree_catalyst_creates_worktree_mock_edit_diff_captured() {
        // Set up a real (but temporary) git repo.
        let repo_dir = std::env::temp_dir().join(format!("a2-test-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo(&repo_dir).await;

        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        // Create an isolated worktree.
        let worktree_path = catalyst.create_worktree().await.unwrap();
        let branch_name = worktree_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Mock edit: overwrite the tracked file with new content.
        fs::write(
            worktree_path.join("README.md"),
            "# repo\nmodified by mock edit\n",
        )
        .unwrap();

        // git diff should capture the change.
        let diff = catalyst.capture_diff(&worktree_path).await.unwrap();

        catalyst.cleanup_worktree(&worktree_path, &branch_name).await;
        let _ = fs::remove_dir_all(&repo_dir);

        assert!(!diff.trim().is_empty(), "diff must be non-empty after mock edit");
        assert!(
            diff.contains("+modified by mock edit"),
            "diff must contain the inserted line; got:\n{diff}"
        );
    }
}
