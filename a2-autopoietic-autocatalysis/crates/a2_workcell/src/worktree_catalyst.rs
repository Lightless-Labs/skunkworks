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
    /// Git ref to branch worktrees from (default: "HEAD").
    base_ref: String,
}

impl WorktreeCatalyst {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            id: CatalystId::new(),
            workspace_root,
            base_ref: "HEAD".into(),
        }
    }

    /// Create a catalyst that branches worktrees from a specific ref (tag, commit, branch).
    pub fn with_base_ref(workspace_root: PathBuf, base_ref: impl Into<String>) -> Self {
        Self {
            id: CatalystId::new(),
            workspace_root,
            base_ref: base_ref.into(),
        }
    }

    /// Create a temporary git worktree and return its path.
    async fn create_worktree(&self) -> A2Result<PathBuf> {
        let branch_name = format!("a2-wc-{}", uuid::Uuid::now_v7());
        let worktree_path = std::env::temp_dir().join(&branch_name);

        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch_name])
            .arg(&worktree_path)
            .arg(&self.base_ref)
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

    async fn current_head(&self, worktree_path: &Path) -> A2Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(worktree_path)
            .output()
            .await
            .map_err(|e| {
                A2Error::CatalystFailure(self.id.clone(), format!("git rev-parse HEAD: {e}"))
            })?;

        if !output.status.success() {
            return Err(A2Error::CatalystFailure(
                self.id.clone(),
                format!(
                    "git rev-parse HEAD failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Capture all changes from the worktree relative to its original base commit.
    ///
    /// The agent may leave changes unstaged, staged, or committed on the temporary
    /// worktree branch. Staging first plus diffing against the pre-agent base commit
    /// captures all three cases.
    async fn capture_diff(&self, worktree_path: &Path, base_commit: &str) -> A2Result<String> {
        // Stage all changes (including untracked files) so they appear in the diff.
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output()
            .await;

        let output = Command::new("git")
            .args(["diff", "--no-color", base_commit])
            .current_dir(worktree_path)
            .output()
            .await
            .map_err(|e| A2Error::CatalystFailure(self.id.clone(), format!("git diff: {e}")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn compact_verification_excerpt(value: &str, max_chars: usize) -> String {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
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

    fn verification_failure_focus(stdout: &str, stderr: &str) -> Vec<String> {
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("failed")
                    || lower.contains("failures:")
                    || lower.contains("panicked at")
                    || lower.contains("assertion failed")
                    || lower.contains("assertion `")
                    || lower.contains("left:")
                    || lower.contains("right:")
                    || lower.contains("error:")
            })
            .take(20)
            .map(|line| Self::compact_verification_excerpt(line, 300))
            .collect()
    }

    fn failing_tests_from_output(stdout: &str, stderr: &str) -> Vec<String> {
        stdout
            .lines()
            .chain(stderr.lines())
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("test ")
                    .and_then(|rest| rest.strip_suffix(" ... FAILED"))
                    .map(str::to_string)
            })
            .collect()
    }

    fn worktree_project_root(&self, worktree_path: &Path) -> PathBuf {
        if worktree_path.join("Cargo.toml").exists() {
            return worktree_path.to_path_buf();
        }

        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&self.workspace_root)
            .output()
            && output.status.success()
        {
            let git_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            let git_root = git_root.canonicalize().unwrap_or(git_root);
            let workspace_root = self
                .workspace_root
                .canonicalize()
                .unwrap_or_else(|_| self.workspace_root.clone());
            if let Ok(relative_project) = workspace_root.strip_prefix(git_root) {
                let candidate = worktree_path.join(relative_project);
                if candidate.join("Cargo.toml").exists() {
                    return candidate;
                }
            }
        }

        worktree_path.to_path_buf()
    }

    async fn run_task_verifications(
        &self,
        task: &TaskContract,
        worktree_path: &Path,
    ) -> A2Result<(TestResults, Vec<ExternalVerification>)> {
        if task.verification_commands.is_empty() {
            return Ok((
                TestResults {
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    details: vec![],
                },
                vec![],
            ));
        }

        let project_root = self.worktree_project_root(worktree_path);
        let mut details = Vec::new();
        let mut verifications = Vec::new();
        for spec in &task.verification_commands {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&spec.command)
                .current_dir(&project_root)
                .output()
                .await
                .map_err(|e| {
                    A2Error::CatalystFailure(
                        self.id.clone(),
                        format!("task verifier `{}` failed to start: {e}", spec.command),
                    )
                })?;
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let passed = exit_code == Some(spec.expect_exit);
            let output_text = if passed {
                None
            } else {
                Some(Self::compact_verification_excerpt(
                    &format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
                    1_200,
                ))
            };
            details.push(TestDetail {
                name: spec.command.clone(),
                passed,
                output: output_text,
            });
            verifications.push(ExternalVerification {
                passed,
                command: spec.command.clone(),
                exit_code,
                failing_tests: Self::failing_tests_from_output(&stdout, &stderr),
                failure_focus: if passed {
                    vec![]
                } else {
                    Self::verification_failure_focus(&stdout, &stderr)
                },
                stdout_excerpt: Self::compact_verification_excerpt(&stdout, 4_000),
                stderr_excerpt: Self::compact_verification_excerpt(&stderr, 4_000),
                verified_at: Utc::now(),
            });
        }

        let passed = details.iter().filter(|detail| detail.passed).count() as u32;
        let failed = details.iter().filter(|detail| !detail.passed).count() as u32;
        Ok((
            TestResults {
                passed,
                failed,
                skipped: 0,
                details,
            },
            verifications,
        ))
    }

    /// Run a model CLI agent in the worktree directory.
    /// Returns (parsed_text, raw_stdout, stderr_text, tokens_in, tokens_out).
    async fn run_agent(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<(String, String, String, u64, u64)> {
        if matches!(
            network_policy,
            Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))
        ) {
            let policy_name = match network_policy {
                Some(NetworkPolicy::Isolated) => "Isolated",
                Some(NetworkPolicy::AllowList(_)) => "AllowList",
                _ => unreachable!(),
            };
            return Err(A2Error::CatalystFailure(
                self.id.clone(),
                format!(
                    "network_policy={policy_name} prevents launching provider `{provider_id}` from the worktree catalyst until an audited network sandbox/provider allowlist is implemented; run with network_policy=Open only when unrestricted network access is intentional"
                ),
            ));
        }
        #[cfg(test)]
        if provider_id == "mock" {
            return self.run_mock_agent(worktree_path).await;
        }
        match provider_id {
            "claude" => self.run_claude(model_id, prompt, worktree_path).await,
            "codex" => self.run_codex(model_id, prompt, worktree_path).await,
            "gemini" => self.run_gemini(model_id, prompt, worktree_path).await,
            "opencode" => self.run_opencode(model_id, prompt, worktree_path).await,
            "pi" => self.run_pi(model_id, prompt, worktree_path).await,
            other => Err(A2Error::CatalystFailure(
                self.id.clone(),
                format!("worktree catalyst doesn't support provider: {other}"),
            )),
        }
    }

    /// Mock agent used in tests: appends a comment to `src/lib.rs` in the worktree.
    #[cfg(test)]
    async fn run_mock_agent(
        &self,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
        let lib_path = worktree_path.join("src").join("lib.rs");
        let mut content = std::fs::read_to_string(&lib_path).map_err(|e| {
            A2Error::CatalystFailure(self.id.clone(), format!("mock agent read src/lib.rs: {e}"))
        })?;
        content.push_str("// mock agent modification\n");
        std::fs::write(&lib_path, &content).map_err(|e| {
            A2Error::CatalystFailure(self.id.clone(), format!("mock agent write src/lib.rs: {e}"))
        })?;
        let raw = "mock agent ran successfully".to_string();
        Ok((raw.clone(), raw, String::new(), 10, 5))
    }

    async fn run_claude(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
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
            .env("PWD", worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("claude: {e}")))?;

        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut text = String::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;

        for line in raw_stdout.lines() {
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

        Ok((text, raw_stdout, stderr, tokens_in, tokens_out))
    }

    async fn run_codex(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
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
            .env("PWD", worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("codex: {e}")))?;

        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut text = String::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;

        for line in raw_stdout.lines() {
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

        Ok((text, raw_stdout, stderr, tokens_in, tokens_out))
    }

    async fn run_gemini(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
        let output = Command::new("gemini")
            .args([
                "-p", prompt, "--model", model_id, "-s", "false", "-y", "-o", "text",
            ])
            .current_dir(worktree_path)
            .env("PWD", worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("gemini: {e}")))?;

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        // Gemini text mode doesn't expose token counts; raw_stdout == text
        Ok((text.clone(), text, stderr, 0, 0))
    }

    async fn run_opencode(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
        let output = Command::new("opencode")
            .args(["run", "--format", "json", "--model", model_id, "--dir"])
            .arg(worktree_path)
            .arg(prompt)
            .current_dir(worktree_path)
            .env("PWD", worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("opencode: {e}")))?;

        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let mut text = String::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;

        for line in raw_stdout.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(t) = v
                    .get("part")
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                {
                    text.push_str(t);
                }
                if v.get("type").and_then(|t| t.as_str()) == Some("step_finish")
                    && let Some(tokens) = v.get("part").and_then(|p| p.get("tokens"))
                {
                    tokens_in += tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
                    tokens_out += tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                }
            }
        }

        Ok((text, raw_stdout, stderr, tokens_in, tokens_out))
    }

    async fn run_pi(
        &self,
        model_id: &str,
        prompt: &str,
        worktree_path: &Path,
    ) -> A2Result<(String, String, String, u64, u64)> {
        let output = Command::new("pi")
            .args([
                "--model",
                model_id,
                "--no-session",
                "--mode",
                "json",
                "--print",
            ])
            .arg(prompt)
            .current_dir(worktree_path)
            .env("PWD", worktree_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(format!("pi: {e}")))?;

        let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let mut message = format!(
                "pi provider model `{model_id}` exited with status {}",
                output.status
            );
            if !stderr.trim().is_empty() {
                message.push_str(": ");
                message.push_str(stderr.trim());
            }
            return Err(A2Error::ProviderError(message));
        }

        let (text, tokens_in, tokens_out) = Self::parse_pi_jsonl(&raw_stdout);

        Ok((text, raw_stdout, stderr, tokens_in, tokens_out))
    }

    fn parse_pi_jsonl(jsonl: &str) -> (String, u64, u64) {
        let mut text = String::new();
        let mut tokens_in = 0;
        let mut tokens_out = 0;

        for line in jsonl.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let event_type = v.get("type").and_then(|t| t.as_str());
            if event_type != Some("turn_end") && event_type != Some("agent_end") {
                continue;
            }
            let Some(message) = v.get("message").or_else(|| {
                v.get("messages")
                    .and_then(|messages| messages.as_array())
                    .and_then(|messages| messages.last())
            }) else {
                continue;
            };
            if message.get("role").and_then(|role| role.as_str()) != Some("assistant") {
                continue;
            }
            if let Some(content) = message
                .get("content")
                .and_then(|content| content.as_array())
            {
                text.clear();
                for block in content {
                    if block.get("type").and_then(|ty| ty.as_str()) == Some("text")
                        && let Some(block_text) =
                            block.get("text").and_then(|block_text| block_text.as_str())
                    {
                        text.push_str(block_text);
                    }
                }
            }
            if let Some(usage) = message.get("usage") {
                tokens_in = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0)
                    + usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0)
                    + usage
                        .get("cacheWrite")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                tokens_out = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
            }
        }

        (text, tokens_in, tokens_out)
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
        let mut prompt = format!(
            "You are working on the A² project.\n\n\
             # Task: {}\n\n\
             {}\n\n\
             ## Instructions\n\n\
             Read the relevant files, make the necessary changes directly, and run \
             `cargo check` plus relevant `cargo test` commands. A² may run additional \
             task-specific verifier commands after your changes before accepting the \
             patch. Do not produce a diff — edit the files directly.\n\n\
             Keep changes minimal and focused on the task.",
            task.title, task.description
        );

        if !task.acceptance_criteria.is_empty() {
            prompt.push_str("\n\n## Acceptance Criteria\n\n");
            for criterion in &task.acceptance_criteria {
                prompt.push_str(&format!("- {criterion}\n"));
            }
        }

        if task.no_external_solution_search {
            prompt.push_str(
                "\n\n## Benchmark Integrity\n\n\
                 - Do not search GitHub, public issue trackers, public pull requests, \n\
                 public patches, or solution writeups for this task.\n\
                 - Solve using only the checked-out repository, the task statement, \n\
                 local documentation, and verifier output produced in this worktree.\n\
                 - If you use online documentation, use it only for general API \n\
                 reference, not task-specific solutions.\n",
            );
        }

        prompt.push_str("\n\n## Workspace Structure\n\n");
        prompt.push_str(self.workspace_structure());

        if !context.relevant_files.is_empty() {
            prompt.push_str("\n## Relevant Files\n\n");
            for file in &context.relevant_files {
                // Try to strip the workspace_root to make the paths relative and shorter
                let path = file.strip_prefix(&self.workspace_root).unwrap_or(file);
                prompt.push_str(&format!("- {}\n", path.display()));
            }
        }

        if !context.retrieved_motifs.is_empty() {
            prompt.push_str(
                "\n## Prior Attempts on This Task\n\n\
                 Earlier workcell runs on this same task produced the results below. \
                 Treat `external_verification` failures as authoritative acceptance \
                 criteria for the next attempt, even when they reveal failures beyond \
                 the original task description. If prior attempts failed, fix every \
                 failing test named in `failure_focus` and try a different approach. \
                 If an `anti_repeat_retry` warning appears, do not repeat the prior \
                 touched-file set alone; inspect and address the unresolved verifier \
                 files. If prior attempts partially succeeded, build on what worked.\n\n",
            );
            for motif in &context.retrieved_motifs {
                prompt.push_str(&format!("- {motif}\n"));
            }
        }

        prompt
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
        context: &ContextPack,
        model: &dyn ModelProvider,
    ) -> A2Result<PatchBundle> {
        let worktree_path = self.create_worktree().await?;
        let branch_name = worktree_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let base_commit = self.current_head(&worktree_path).await?;
        let prompt = self.build_prompt(task, context);

        // Run the agent in the worktree
        let result = self
            .run_agent(
                model.provider_id(),
                model.model_id(),
                &prompt,
                &worktree_path,
                task.network_policy.as_ref(),
            )
            .await;

        let (rationale, raw_stdout, stderr, tokens_in, tokens_out) = match result {
            Ok(r) => r,
            Err(e) => {
                self.cleanup_worktree(&worktree_path, &branch_name).await;
                return Err(e);
            }
        };

        // Capture what the agent actually changed
        let diff = self.capture_diff(&worktree_path, &base_commit).await?;

        let verification_result = self.run_task_verifications(task, &worktree_path).await;

        // Clean up
        self.cleanup_worktree(&worktree_path, &branch_name).await;

        let (test_results, worktree_verifications) = verification_result?;

        if diff.trim().is_empty() {
            let mut msg = "agent made no changes to the worktree — the worktree agent must edit the correct file and the diff must apply cleanly".to_string();
            if !rationale.trim().is_empty() {
                msg.push_str("\n--- model output ---\n");
                msg.push_str(&rationale);
            }
            const MAX_RAW: usize = 4096;
            let raw_snippet = if raw_stdout.len() > MAX_RAW {
                // Find a safe char boundary so we don't panic on multibyte sequences.
                let mut end = MAX_RAW;
                while !raw_stdout.is_char_boundary(end) {
                    end -= 1;
                }
                format!(
                    "{} … [truncated {} bytes]",
                    &raw_stdout[..end],
                    raw_stdout.len() - end
                )
            } else {
                raw_stdout.clone()
            };
            msg.push_str("\n--- model stdout (raw) ---\n");
            msg.push_str(&raw_snippet);
            if !stderr.trim().is_empty() {
                msg.push_str("\n--- model stderr ---\n");
                msg.push_str(&stderr);
            }
            return Err(A2Error::CatalystFailure(self.id.clone(), msg));
        }

        Ok(PatchBundle {
            id: PatchId::new(),
            task_id: task.id.clone(),
            workcell_id: WorkcellId::new(),
            diff,
            rationale,
            test_results,
            worktree_verifications,
            network_policy_enforced: match &task.network_policy {
                Some(NetworkPolicy::Open) | None => None,
                enforced => enforced.clone(),
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
        let base_commit = catalyst.current_head(&worktree_path).await.unwrap();
        let diff = catalyst
            .capture_diff(&worktree_path, &base_commit)
            .await
            .unwrap();

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
    async fn worktree_catalyst_captures_agent_commits() {
        let repo_dir = std::env::temp_dir().join(format!("a2-test-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo(&repo_dir).await;

        let catalyst = WorktreeCatalyst::new(repo_dir.clone());
        let worktree_path = catalyst.create_worktree().await.unwrap();
        let branch_name = worktree_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let base_commit = catalyst.current_head(&worktree_path).await.unwrap();

        fs::write(
            worktree_path.join("README.md"),
            "# repo\ncommitted by mock agent\n",
        )
        .unwrap();
        for args in [vec!["add", "README.md"], vec!["commit", "-m", "agent edit"]] {
            Command::new("git")
                .args(&args)
                .current_dir(&worktree_path)
                .output()
                .await
                .unwrap();
        }

        let diff = catalyst
            .capture_diff(&worktree_path, &base_commit)
            .await
            .unwrap();

        catalyst
            .cleanup_worktree(&worktree_path, &branch_name)
            .await;
        let _ = fs::remove_dir_all(&repo_dir);

        assert!(
            diff.contains("+committed by mock agent"),
            "diff must include committed worktree changes; got:\n{diff}"
        );
    }

    #[test]
    fn verifier_runs_from_nested_project_root_when_git_worktree_is_monorepo_root() {
        let git_root = std::env::temp_dir().join(format!("a2-monorepo-{}", uuid::Uuid::now_v7()));
        let project = git_root.join("a2-autopoietic-autocatalysis");
        let candidate_root =
            std::env::temp_dir().join(format!("a2-candidate-{}", uuid::Uuid::now_v7()));
        let candidate_project = candidate_root.join("a2-autopoietic-autocatalysis");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&candidate_project).unwrap();
        fs::write(project.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(candidate_project.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&git_root)
            .output()
            .unwrap();

        let catalyst = WorktreeCatalyst::new(project.clone());

        assert_eq!(
            catalyst.worktree_project_root(&candidate_root),
            candidate_project
        );
    }

    #[test]
    fn prompt_renders_acceptance_criteria_without_leaking_verifier_commands() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir);
        let task = TaskContract {
            id: TaskId::new(),
            title: "Fix retry contract".into(),
            description: "Use verifier-derived criteria.".into(),
            acceptance_criteria: vec![
                "Original acceptance remains".into(),
                "Prior external verification must pass: cargo test -p a2ctl".into(),
            ],
            verification_commands: vec![TaskVerificationCommand {
                command: "cargo test -p a2ctl hidden_test".into(),
                expect_exit: 0,
            }],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            no_external_solution_search: false,
            network_policy: None,
            created_at: Utc::now(),
        };
        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![std::path::PathBuf::from("crates/a2ctl/src/main.rs")],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        };

        let prompt = catalyst.build_prompt(&task, &context);

        assert!(prompt.contains("## Acceptance Criteria"));
        assert!(prompt.contains("- Original acceptance remains"));
        assert!(prompt.contains("- Prior external verification must pass: cargo test -p a2ctl"));
        assert!(prompt.contains("A² may run additional task-specific verifier commands"));
        assert!(!prompt.contains("## Task-Specific Verification Commands"));
        assert!(!prompt.contains("hidden_test"));
        assert!(prompt.contains("## Relevant Files"));
        assert!(prompt.contains("- crates/a2ctl/src/main.rs"));
    }

    #[test]
    fn prompt_includes_benchmark_integrity_guard_when_requested() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir);
        let mut task = TaskContract {
            id: TaskId::new(),
            title: "Solve benchmark task".into(),
            description: "Fix the benchmark failure.".into(),
            acceptance_criteria: vec![],
            verification_commands: vec![],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "senior-swe-bench".into(),
            },
            no_external_solution_search: true,
            network_policy: None,
            created_at: Utc::now(),
        };
        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        };

        let prompt = catalyst.build_prompt(&task, &context);
        assert!(prompt.contains("## Benchmark Integrity"));
        assert!(prompt.contains("Do not search GitHub"));
        assert!(prompt.contains("public pull requests"));
        assert!(prompt.contains("not task-specific solutions"));

        task.no_external_solution_search = false;
        let prompt_without_guard = catalyst.build_prompt(&task, &context);
        assert!(!prompt_without_guard.contains("## Benchmark Integrity"));
        assert!(!prompt_without_guard.contains("Do not search GitHub"));
    }

    #[tokio::test]
    async fn isolated_network_policy_refuses_external_agent_launch_before_spawn() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        let error = catalyst
            .run_agent(
                "opencode",
                "model",
                "prompt",
                &repo_dir,
                Some(&NetworkPolicy::Isolated),
            )
            .await
            .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("network_policy=Isolated prevents launching provider `opencode`"));
        assert!(message.contains("audited network sandbox/provider allowlist"));
    }

    #[tokio::test]
    async fn mock_agent_cannot_bypass_restricted_network_policy() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        let error = catalyst
            .run_agent(
                "mock",
                "model",
                "prompt",
                &repo_dir,
                Some(&NetworkPolicy::Isolated),
            )
            .await
            .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("network_policy=Isolated prevents launching provider `mock`"));
        assert!(message.contains("audited network sandbox/provider allowlist"));
    }

    #[tokio::test]
    async fn mock_agent_cannot_bypass_allowlist_network_policy() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        let error = catalyst
            .run_agent(
                "mock",
                "model",
                "prompt",
                &repo_dir,
                Some(&NetworkPolicy::AllowList(vec![
                    "https://api.openai.com".to_string(),
                ])),
            )
            .await
            .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("network_policy=AllowList prevents launching provider `mock`"));
        assert!(message.contains("audited network sandbox/provider allowlist"));
    }

    #[tokio::test]
    async fn execute_with_mock_refuses_restricted_network_policy_before_patch_bundle() {
        let repo_dir =
            std::env::temp_dir().join(format!("a2-policy-execute-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo_with_rust_project(&repo_dir).await;

        let catalyst = WorktreeCatalyst::new(repo_dir.clone());
        let task = TaskContract {
            id: TaskId::new(),
            title: "Restricted policy cannot produce patch".into(),
            description: "Mock worktree execution must not bypass network policy.".into(),
            acceptance_criteria: vec![],
            verification_commands: vec![],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            no_external_solution_search: true,
            network_policy: Some(NetworkPolicy::Isolated),
            created_at: Utc::now(),
        };
        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![repo_dir.join("src/lib.rs")],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        };
        let model = MockModelProvider {
            provider_id: "mock",
            model_id: "mock",
        };

        let error = catalyst.execute(&task, &context, &model).await.unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("network_policy=Isolated prevents launching provider `mock`"));
        assert!(message.contains("audited network sandbox/provider allowlist"));
    }

    #[tokio::test]
    async fn allowlist_network_policy_refuses_external_agent_launch_before_spawn() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir.clone());

        let error = catalyst
            .run_agent(
                "pi",
                "provider/model",
                "prompt",
                &repo_dir,
                Some(&NetworkPolicy::AllowList(vec![
                    "https://api.openai.com".to_string(),
                ])),
            )
            .await
            .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("network_policy=AllowList prevents launching provider `pi`"));
        assert!(message.contains("network_policy=Open"));
    }

    #[test]
    fn pi_jsonl_parser_extracts_final_text_and_usage() {
        let jsonl = r#"{"type":"turn_end","message":{"role":"assistant","content":[{"type":"text","text":"done"}],"usage":{"input":10,"output":20,"cacheRead":30,"cacheWrite":40}}}
"#;

        let (text, tokens_in, tokens_out) = WorktreeCatalyst::parse_pi_jsonl(jsonl);

        assert_eq!(text, "done");
        assert_eq!(tokens_in, 80);
        assert_eq!(tokens_out, 20);
    }

    #[test]
    fn prompt_treats_prior_external_verification_as_authoritative() {
        let repo_dir = std::env::temp_dir().join(format!("a2-prompt-{}", uuid::Uuid::now_v7()));
        let catalyst = WorktreeCatalyst::new(repo_dir);
        let task = TaskContract {
            id: TaskId::new(),
            title: "Fix visible bug".into(),
            description: "Fix `cargo test -p a2_core test_fibonacci`.".into(),
            acceptance_criteria: vec![],
            verification_commands: vec![],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            no_external_solution_search: false,
            network_policy: None,
            created_at: Utc::now(),
        };
        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![],
            prior_attempts: vec![LineageId::new()],
            retrieved_motifs: vec![
                "attempt 1 [opencode/minimax]\n  external_verification:\n    result: FAIL\n    failure_focus: tests::hidden_regression ... FAILED"
                    .into(),
            ],
        };

        let prompt = catalyst.build_prompt(&task, &context);

        assert!(prompt.contains("Treat `external_verification` failures as authoritative"));
        assert!(prompt.contains("fix every failing test named in `failure_focus`"));
        assert!(prompt.contains("If an `anti_repeat_retry` warning appears"));
        assert!(prompt.contains("do not repeat the prior"));
        assert!(prompt.contains("tests::hidden_regression"));
    }

    #[tokio::test]
    async fn task_specific_verifier_failure_is_captured_in_patch() {
        let repo_dir = std::env::temp_dir().join(format!("a2-verifier-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo_with_rust_project(&repo_dir).await;

        let catalyst = WorktreeCatalyst::new(repo_dir.clone());
        let task = TaskContract {
            id: TaskId::new(),
            title: "Capture verifier failure".into(),
            description: "Make a change, then run a failing verifier.".into(),
            acceptance_criteria: vec![],
            verification_commands: vec![TaskVerificationCommand {
                command: "echo thread panicked at crates/a2ctl/src/main.rs:42; exit 7".into(),
                expect_exit: 0,
            }],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "test".into(),
            },
            no_external_solution_search: false,
            network_policy: None,
            created_at: Utc::now(),
        };
        let context = ContextPack {
            germline_version: GermlineVersion::new(),
            relevant_files: vec![repo_dir.join("src/lib.rs")],
            prior_attempts: vec![],
            retrieved_motifs: vec![],
        };
        let model = MockModelProvider {
            provider_id: "mock",
            model_id: "mock",
        };

        let patch = catalyst.execute(&task, &context, &model).await.unwrap();

        assert_eq!(patch.test_results.passed, 0);
        assert_eq!(patch.test_results.failed, 1);
        assert_eq!(patch.worktree_verifications.len(), 1);
        let verification = &patch.worktree_verifications[0];
        assert!(!verification.passed);
        assert_eq!(verification.exit_code, Some(7));
        assert!(
            verification
                .failure_focus
                .iter()
                .any(|line| line.contains("crates/a2ctl/src/main.rs"))
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
            verification_commands: vec![],
            budget: Budget {
                max_tokens: 10_000,
                max_duration_secs: 60,
                max_calls: 4,
            },
            priority: Priority::Normal,
            source: TaskSource::External {
                origin: "integration-test".into(),
            },
            no_external_solution_search: false,
            network_policy: None,
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
