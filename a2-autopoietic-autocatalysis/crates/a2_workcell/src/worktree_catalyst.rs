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
            .map_err(|e| {
                A2Error::CatalystFailure(self.id.clone(), format!("git worktree add: {e}"))
            })?;

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
        #[cfg(test)]
        if provider_id == "mock" {
            return self.run_mock_agent(worktree_path).await;
        }
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

    /// Mock agent used in tests: appends a comment to `src/lib.rs` in the worktree.
    #[cfg(test)]
    async fn run_mock_agent(&self, worktree_path: &Path) -> A2Result<(String, u64, u64)> {
        let lib_path = worktree_path.join("src").join("lib.rs");
        let mut content = std::fs::read_to_string(&lib_path).map_err(|e| {
            A2Error::CatalystFailure(self.id.clone(), format!("mock agent read src/lib.rs: {e}"))
        })?;
        content.push_str("// mock agent modification\n");
        std::fs::write(&lib_path, &content).map_err(|e| {
            A2Error::CatalystFailure(self.id.clone(), format!("mock agent write src/lib.rs: {e}"))
        })?;
        Ok(("mock agent ran successfully".into(), 10, 5))
    }

    async fn run_claude(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, u64, u64)> {
        let output = Command::new("claude")
            .args([
                "-p",
                prompt,
                "--model",
                model_id,
                "--output-format",
                "stream-json",
                "--verbose",
                "--max-turns",
                "30",
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
                if let Some(delta) = v
                    .get("delta")
                    .and_then(|d| d.get("text"))
                    .and_then(|t| t.as_str())
                {
                    text.push_str(delta);
                }
                // Collect usage
                if v.get("type").and_then(|t| t.as_str()) == Some("result")
                    && let Some(usage) = v.get("usage")
                {
                    tokens_in = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    tokens_out = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
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
                "exec",
                prompt,
                "-m",
                model_id,
                "-c",
                "model_reasoning_effort=\"high\"",
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
                if let Some(msg) = v
                    .get("item")
                    .and_then(|i| i.get("text"))
                    .and_then(|t| t.as_str())
                {
                    text.push_str(msg);
                }
                if v.get("type").and_then(|t| t.as_str()) == Some("turn.completed")
                    && let Some(usage) = v.get("usage")
                {
                    tokens_in = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    tokens_out = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
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
                "-p", prompt, "--model", model_id, "-s", "false", "-y", "-o", "text",
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
            .run_agent(
                model.provider_id(),
                model.model_id(),
                &prompt,
                &worktree_path,
            )
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
                format!(
                    "agent made no changes to the worktree\n--- model stdout ---\n{rationale}"
                ),
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
    struct MockModelProvider {
        provider_id: &'static str,
        model_id: &'static str,
    }

    #[async_trait::async_trait]
    impl ModelProvider for MockModelProvider {
        async fn generate(
            &self,
            _prompt: &str,
            _system: Option<&str>,
        ) -> A2Result<GenerateResponse> {
            Ok(GenerateResponse {
                text: "mock response".into(),
                tokens_in: 0,
                tokens_out: 0,
            })
        }

        fn provider_id(&self) -> &str {
            self.provider_id
        }

        fn model_id(&self) -> &str {
            self.model_id
        }
    }

    /// Bootstrap a minimal git repo with a Rust package so `cargo check` works.
    async fn init_git_repo_with_rust_project(dir: &Path) {
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
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"mock-selfmod\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/lib.rs"), "// initial\npub fn hello() {}\n").unwrap();
        for args in [vec!["add", "."], vec!["commit", "-m", "initial"]] {
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .output()
                .await
                .unwrap();
        }
    }

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

        catalyst
            .cleanup_worktree(&worktree_path, &branch_name)
            .await;
        let _ = fs::remove_dir_all(&repo_dir);

        assert!(
            !diff.trim().is_empty(),
            "diff must be non-empty after mock edit"
        );
        assert!(
            diff.contains("+modified by mock edit"),
            "diff must contain the inserted line; got:\n{diff}"
        );
    }

    #[tokio::test]
    async fn full_self_modification_pipeline_create_task_execute_diff_apply_cargo_check() {
        // --- 1. Bootstrap: temp git repo with a minimal Rust project -------
        let repo_dir = std::env::temp_dir().join(format!("a2-selfmod-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo_with_rust_project(&repo_dir).await;

        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        // --- 2. Create task -------------------------------------------------
        let task = TaskContract {
            id: TaskId::new(),
            title: "Add comment to src/lib.rs".into(),
            description: "Append a comment line to the lib file.".into(),
            acceptance_criteria: vec!["src/lib.rs contains a new comment".into()],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "integration-test".into(),
            },
            created_at: Utc::now(),
        };

        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![repo_dir.join("src/lib.rs")],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        };

        // --- 3. Run worktree catalyst with mock model provider --------------
        let model = MockModelProvider {
            provider_id: "mock",
            model_id: "mock-model",
        };
        let patch = catalyst
            .execute(&task, &context, &model)
            .await
            .expect("execute must succeed");

        // --- 4. Verify diff captured ----------------------------------------
        assert!(!patch.diff.trim().is_empty(), "diff must be non-empty");
        assert!(
            patch.diff.contains("mock agent modification"),
            "diff must contain the mock agent change; got:\n{}",
            patch.diff
        );
        assert_eq!(patch.task_id, task.id);
        assert_eq!(patch.model_attribution.provider, "mock");

        // --- 5. Apply the diff to the main repo ----------------------------
        let mut apply_child = Command::new("git")
            .args(["apply"])
            .current_dir(&repo_dir)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("git apply failed to spawn");
        {
            use tokio::io::AsyncWriteExt;
            apply_child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(patch.diff.as_bytes())
                .await
                .unwrap();
        }
        let apply_status = apply_child.wait().await.unwrap();
        assert!(
            apply_status.success(),
            "git apply must succeed; diff was:\n{}",
            patch.diff
        );

        // --- 6. Verify build passes ----------------------------------------
        let check = Command::new("cargo")
            .args(["check"])
            .current_dir(&repo_dir)
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .await
            .expect("cargo check failed to spawn");
        assert!(
            check.status.success(),
            "cargo check must pass after applying diff;\nstderr: {}",
            String::from_utf8_lossy(&check.stderr)
        );

        let _ = fs::remove_dir_all(&repo_dir);
    }
}
