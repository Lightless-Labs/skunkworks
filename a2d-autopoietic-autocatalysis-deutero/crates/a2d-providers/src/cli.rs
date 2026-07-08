//! CLI-based providers: invoke models through their CLI tools.
//!
//! This wraps codex, gemini, and opencode CLIs as Provider implementations.
//! CLI providers are the simplest integration path — no SDK, just subprocess.

use a2d_core::observer::ToolEvent;
use a2d_core::provider::{
    InvocationRequest, InvocationResponse, Provider, ProviderError, TokenUsage,
};
use std::process::Command;

/// A provider that invokes a model via a CLI subprocess.
pub struct CliProvider {
    name: String,
    command: String,
    args_builder: Box<dyn Fn(&InvocationRequest) -> Vec<String> + Send + Sync>,
    output_parser: Box<dyn Fn(&str) -> (String, Option<String>) + Send + Sync>,
}

impl CliProvider {
    /// Create a Codex CLI provider.
    ///
    /// Models: "gpt-5.4" (with reasoning_effort), "gpt-5.4-mini", "gpt-5.3-spark".
    /// Empty string uses account default.
    /// reasoning_effort: none/minimal/low/medium/high/xhigh (only for gpt-5.4).
    pub fn codex(model: &str, reasoning_effort: Option<&str>) -> Self {
        let model = model.to_string();
        let reasoning = reasoning_effort.map(|r| r.to_string());
        let name = if model.is_empty() {
            "codex/default".to_string()
        } else {
            format!("codex/{model}")
        };
        Self {
            name,
            command: "codex".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                let mut args = vec![
                    "exec".to_string(),
                    format!("{}\n\n{}", req.system, req.prompt),
                ];
                if !model.is_empty() {
                    args.push("-m".to_string());
                    args.push(model.clone());
                }
                if let Some(ref effort) = reasoning {
                    args.push("-c".to_string());
                    args.push(format!("model_reasoning_effort=\"{effort}\""));
                }
                args.extend([
                    "--sandbox".to_string(),
                    "read-only".to_string(),
                    "--skip-git-repo-check".to_string(),
                    "--ephemeral".to_string(),
                    "--ignore-user-config".to_string(),
                    "--ignore-rules".to_string(),
                ]);
                args
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create a Gemini CLI provider.
    pub fn gemini(model: &str) -> Self {
        let model = model.to_string();
        Self {
            name: format!("gemini/{model}"),
            command: "gemini".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                vec![
                    "-p".to_string(),
                    format!("{}\n\n{}", req.system, req.prompt),
                    "--model".to_string(),
                    model.clone(),
                    "--sandbox".to_string(),
                    "-o".to_string(),
                    "text".to_string(),
                ]
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create a Pi CLI provider.
    ///
    /// Pi is the repo-maintenance/coding-agent harness used for the outer
    /// project loop. Run it in non-interactive ephemeral mode and disable tools
    /// for typed artifact-production roles; A²D applies any proposed changes
    /// through its own patchset gates rather than allowing direct provider
    /// filesystem mutation.
    pub fn pi(model: Option<&str>) -> Self {
        let model = model.map(str::to_string);
        let name = model
            .as_ref()
            .map(|model| format!("pi/{model}"))
            .unwrap_or_else(|| "pi/default".to_string());
        Self {
            name,
            command: "pi".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                let mut args = vec![
                    "--print".to_string(),
                    "--no-session".to_string(),
                    "--no-tools".to_string(),
                    "--no-context-files".to_string(),
                    "--no-extensions".to_string(),
                    "--no-skills".to_string(),
                    "--no-prompt-templates".to_string(),
                    "--no-themes".to_string(),
                    "--system-prompt".to_string(),
                    req.system.clone(),
                ];
                if let Some(model) = &model {
                    args.push("--model".to_string());
                    args.push(model.clone());
                }
                args.push(req.prompt.clone());
                args
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create an OpenCode CLI provider.
    ///
    /// Uses `--format json` to get clean NDJSON output without ANSI codes.
    /// Parses text events from the JSON stream.
    pub fn opencode(model: &str) -> Self {
        let model = model.to_string();
        Self {
            name: format!("opencode/{model}"),
            command: "opencode".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                vec![
                    "run".to_string(),
                    "--model".to_string(),
                    model.clone(),
                    "--pure".to_string(),
                    "--agent".to_string(),
                    opencode_artifact_agent_name().to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                    format!("SYSTEM: {}\n\nUSER: {}", req.system, req.prompt),
                ]
            }),
            output_parser: Box::new(|output: &str| (parse_opencode_output(output), None)),
        }
    }
}

fn parse_opencode_output(output: &str) -> String {
    // OpenCode's `--format json` emits NDJSON. The event shape has changed
    // across versions: current releases put assistant text at `/part/text`,
    // older examples used top-level `/text`, and tool calls report file writes
    // under `/part/state/input/content`. Collect the useful assistant channels
    // and fall back to written file content when the final message is only a
    // generic acknowledgement like "Done.".
    let mut text = String::new();
    let mut written_contents = Vec::new();

    for line in output.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if v.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = v.pointer("/part/text").and_then(|t| t.as_str()) {
                text.push_str(t);
            } else if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
        }

        let part_tool = v.pointer("/part/tool").and_then(|t| t.as_str());
        let top_level_tool = v.get("tool").and_then(|t| t.as_str());
        if (part_tool == Some("write") || top_level_tool == Some("write"))
            && let Some(content) = v
                .pointer("/part/state/input/content")
                .or_else(|| v.pointer("/state/input/content"))
                .and_then(|content| content.as_str())
        {
            written_contents.push(content.to_string());
        }
    }

    let trimmed = text.trim();
    if let Some(last_written) = written_contents.last()
        && (trimmed.is_empty() || is_generic_opencode_completion(trimmed))
    {
        return last_written.clone();
    }

    text
}

fn opencode_artifact_agent_name() -> &'static str {
    "a2d-artifact-no-tools"
}

fn opencode_tool_denial_config() -> String {
    serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "agent": {
            opencode_artifact_agent_name(): {
                "description": "A²D artifact-only provider role. All tools are denied; emit the requested JSON/text artifact in stdout.",
                "permission": {
                    "*": "deny"
                }
            }
        }
    })
    .to_string()
}

fn is_generic_opencode_completion(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "done"
            | "complete"
            | "completed"
            | "created"
            | "file created"
            | "wrote file"
            | "written"
            | "the file has been created"
            | "the file has been written"
    )
}

fn default_timeout_secs(provider_name: &str) -> u64 {
    if provider_name.contains("zai-coding-plan/glm-5.") {
        900
    } else {
        300
    }
}

pub fn provider_no_public_solution_search_env() -> [(&'static str, &'static str); 5] {
    // Policy-observability only: these flags make the benchmark no-public-
    // solution-search contract visible to provider subprocesses. They do not
    // enforce OS/network isolation.
    [
        ("A2D_PROVIDER_POLICY_ENV_SOURCE", "a2d-cli-provider"),
        ("A2D_GITHUB_SOLUTION_SEARCH_ALLOWED", "false"),
        ("A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN", "true"),
        (
            "A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED",
            "false",
        ),
        (
            "A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN",
            "true",
        ),
    ]
}

pub fn network_configuration_env_vars() -> [&'static str; 16] {
    // Environment scrubbing is defense-in-depth only. It removes common proxy
    // and package-manager network configuration inherited from the parent
    // process, but it is not OS/network namespace isolation.
    a2d_core::process_env::network_configuration_env_vars()
}

pub fn remove_network_configuration_env(command: &mut Command) {
    a2d_core::process_env::remove_network_configuration_env(command);
}

fn isolated_provider_cwd(command: &str) -> Result<std::path::PathBuf, ProviderError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "a2d-provider-{command}-{}-{now}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| {
        ProviderError::InvocationFailed(format!(
            "failed to create isolated provider cwd {}: {e}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

impl Provider for CliProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        let args = (self.args_builder)(request);

        if std::env::var("A2D_TRACE").is_ok_and(|v| !v.is_empty() && v != "0") {
            let total: usize = args.iter().map(|a| a.len()).sum();
            eprintln!(
                "[a2d cli] spawning {} with {} args, total {} bytes",
                self.command,
                args.len(),
                total
            );
            for (i, a) in args.iter().enumerate() {
                let preview: String = a.chars().take(80).collect();
                eprintln!(
                    "[a2d cli]   arg[{}] ({} bytes): {:?}{}",
                    i,
                    a.len(),
                    preview,
                    if a.len() > 80 { "..." } else { "" }
                );
            }
        }

        // CLI coding tools can have filesystem-writing capabilities. Enzymes
        // must communicate artifacts through stdout only; system source changes
        // must go through the architect's SystemPatch + self-sandbox gate. Run
        // each provider in an empty temp cwd so a model cannot mutate the repo
        // directly as a side effect of generation.
        let sandbox_dir = isolated_provider_cwd(&self.command)?;
        if self.command == "opencode" {
            std::fs::write(
                sandbox_dir.join("opencode.json"),
                opencode_tool_denial_config(),
            )
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&sandbox_dir);
                ProviderError::InvocationFailed(format!(
                    "failed to write opencode tool-denial config: {e}"
                ))
            })?;
        }

        let mut command = Command::new(&self.command);
        command
            .args(&args)
            .envs(provider_no_public_solution_search_env())
            .current_dir(&sandbox_dir)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        remove_network_configuration_env(&mut command);

        let mut child = command.spawn().map_err(|e| {
            let _ = std::fs::remove_dir_all(&sandbox_dir);
            ProviderError::InvocationFailed(format!(
                "{} not found or failed to start: {e}",
                self.command
            ))
        })?;

        // Provider timeout — no invocation should silently hang.
        // GLM is often slow-but-productive; give it a longer default window.
        // Override with A2D_PROVIDER_TIMEOUT_SECS env var for explicit experiments.
        let timeout_secs = std::env::var("A2D_PROVIDER_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| default_timeout_secs(&self.name));
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_dir_all(&sandbox_dir);
                        return Err(ProviderError::InvocationFailed(format!(
                            "{} timed out after {}s",
                            self.command,
                            timeout.as_secs()
                        )));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&sandbox_dir);
                    return Err(ProviderError::InvocationFailed(format!(
                        "{} wait failed: {e}",
                        self.command
                    )));
                }
            }
        }

        let output = child.wait_with_output().map_err(|e| {
            let _ = std::fs::remove_dir_all(&sandbox_dir);
            ProviderError::InvocationFailed(format!("{} output read failed: {e}", self.command))
        })?;
        let _ = std::fs::remove_dir_all(&sandbox_dir);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::InvocationFailed(format!(
                "{} exited with {}: {stderr}",
                self.command, output.status
            )));
        }

        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let (text, thinking) = (self.output_parser)(&raw);

        // CLI providers don't give us fine-grained tool events,
        // so we produce a minimal trace: one Think + one Text
        let tool_events = vec![ToolEvent::Think, ToolEvent::Text];

        Ok(InvocationResponse {
            text,
            raw_output: Some(raw),
            tool_events,
            thinking,
            usage: TokenUsage::default(), // CLI doesn't report token usage
        })
    }

    fn supports_thinking(&self) -> bool {
        false // CLI output doesn't include thinking blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_args(provider: &CliProvider) -> Vec<String> {
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };
        (provider.args_builder)(&request)
    }

    #[test]
    fn codex_provider_uses_read_only_ephemeral_artifact_sandbox() {
        let provider = CliProvider::codex("gpt-5.4", Some("low"));
        let args = provider_args(&provider);

        let sandbox_arg = args
            .iter()
            .position(|arg| arg == "--sandbox")
            .expect("codex args should set an explicit sandbox");
        assert_eq!(
            args.get(sandbox_arg + 1).map(String::as_str),
            Some("read-only")
        );
        assert!(args.contains(&"--ephemeral".to_string()));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(args.contains(&"--ignore-user-config".to_string()));
        assert!(args.contains(&"--ignore-rules".to_string()));
    }

    #[test]
    fn codex_provider_does_not_request_full_auto_or_bypass_sandbox() {
        let provider = CliProvider::codex("gpt-5.4", Some("low"));
        let args = provider_args(&provider);

        assert!(!args.contains(&"--full-auto".to_string()));
        assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.contains(&"--dangerously-bypass-hook-trust".to_string()));
        assert!(!args.iter().any(|arg| arg == "--add-dir"));
    }

    #[test]
    fn glm_gets_longer_default_timeout() {
        assert_eq!(
            default_timeout_secs("opencode/zai-coding-plan/glm-5.1"),
            900
        );
        assert_eq!(
            default_timeout_secs("opencode/zai-coding-plan/glm-5.2"),
            900
        );
    }

    #[test]
    fn non_glm_providers_keep_shorter_default_timeout() {
        assert_eq!(default_timeout_secs("opencode/kimi-for-coding/k2p5"), 300);
        assert_eq!(default_timeout_secs("gemini/gemini-3-pro"), 300);
        assert_eq!(default_timeout_secs("pi/default"), 300);
    }

    #[test]
    fn cli_providers_export_no_public_solution_search_policy_env() {
        let env = provider_no_public_solution_search_env()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(
            env.get("A2D_PROVIDER_POLICY_ENV_SOURCE"),
            Some(&"a2d-cli-provider")
        );
        assert_eq!(
            env.get("A2D_GITHUB_SOLUTION_SEARCH_ALLOWED"),
            Some(&"false")
        );
        assert_eq!(
            env.get("A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN"),
            Some(&"true")
        );
        assert_eq!(
            env.get("A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED"),
            Some(&"false")
        );
        assert_eq!(
            env.get("A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN"),
            Some(&"true")
        );
    }

    #[test]
    fn cli_provider_subprocess_receives_no_public_solution_search_policy_env() {
        let provider = CliProvider {
            name: "test/env-provider".to_string(),
            command: "sh".to_string(),
            args_builder: Box::new(|_req| {
                vec![
                    "-c".to_string(),
                    "printf '%s|%s|%s|%s|%s' \"$A2D_PROVIDER_POLICY_ENV_SOURCE\" \"$A2D_GITHUB_SOLUTION_SEARCH_ALLOWED\" \"$A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\" \"$A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED\" \"$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\"".to_string(),
                ]
            }),
            output_parser: Box::new(|output| (output.to_string(), None)),
        };
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let response = provider.invoke(&request).expect("provider env probe");
        assert_eq!(response.text, "a2d-cli-provider|false|true|false|true");
    }

    #[test]
    fn remove_network_configuration_env_drops_explicit_network_env() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("env");
        for key in network_configuration_env_vars() {
            command.env(key, "http://example.invalid:9");
        }
        command.env("A2D_ENV_SCRUB_SENTINEL", "present");
        remove_network_configuration_env(&mut command);

        let output = command.output().expect("env scrub probe should run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        for key in network_configuration_env_vars() {
            assert!(
                !stdout
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}="))),
                "network configuration env var {key} leaked to subprocess: {stdout}"
            );
        }
        assert!(
            stdout
                .lines()
                .any(|line| line == "A2D_ENV_SCRUB_SENTINEL=present")
        );
    }

    #[test]
    fn network_env_scrub_preserves_no_public_solution_search_policy_env() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("env");
        for key in network_configuration_env_vars() {
            command.env(key, "http://example.invalid:9");
        }
        command.envs(provider_no_public_solution_search_env());
        remove_network_configuration_env(&mut command);

        let output = command
            .output()
            .expect("policy env plus network scrub probe should run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        for key in network_configuration_env_vars() {
            assert!(
                !stdout
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}="))),
                "network configuration env var {key} leaked to subprocess: {stdout}"
            );
        }
        for (key, value) in provider_no_public_solution_search_env() {
            assert!(
                stdout.lines().any(|line| line == format!("{key}={value}")),
                "no-public-solution-search policy env {key}={value} missing after network scrub: {stdout}"
            );
        }
    }

    #[test]
    fn pi_provider_is_ephemeral_no_tools_artifact_mode() {
        let provider = CliProvider::pi(None);
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("maintainer"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        assert_eq!(provider.name(), "pi/default");
        let args = (provider.args_builder)(&request);
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--no-session".to_string()));
        assert!(args.contains(&"--no-tools".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
    }

    #[test]
    fn opencode_provider_uses_pure_mode_for_artifact_invocations() {
        let provider = CliProvider::opencode("test/provider");
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("architect"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let args = (provider.args_builder)(&request);
        assert!(args.contains(&"--pure".to_string()));
    }

    #[test]
    fn opencode_provider_selects_tool_denied_artifact_agent() {
        let provider = CliProvider::opencode("test/provider");
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("architect"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let args = (provider.args_builder)(&request);
        let agent_arg = args
            .iter()
            .position(|arg| arg == "--agent")
            .expect("opencode args should select the artifact agent");
        assert_eq!(
            args.get(agent_arg + 1).map(String::as_str),
            Some(opencode_artifact_agent_name())
        );
    }

    #[test]
    fn opencode_tool_denial_config_denies_all_tools_for_artifact_agent() {
        let config: serde_json::Value =
            serde_json::from_str(&opencode_tool_denial_config()).unwrap();
        assert_eq!(
            config
                .pointer("/agent/a2d-artifact-no-tools/permission/*")
                .and_then(|value| value.as_str()),
            Some("deny")
        );
    }

    #[test]
    fn opencode_parser_extracts_current_part_text_shape() {
        let ndjson = r#"{"type":"step_start","part":{"type":"step-start"}}
{"type":"text","part":{"type":"text","text":"{\"output\":\"ok\"}"}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"ok"}"#);
    }

    #[test]
    fn opencode_parser_extracts_legacy_top_level_text_shape() {
        let ndjson = r#"{"type":"text","text":"legacy text"}
"#;

        assert_eq!(parse_opencode_output(ndjson), "legacy text");
    }

    #[test]
    fn opencode_parser_uses_write_content_when_final_text_is_done() {
        let ndjson = r#"{"type":"tool_use","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"/tmp/answer.txt","content":"{\"output\":\"ok\"}"}}}}
{"type":"text","part":{"type":"text","text":"Done."}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"ok"}"#);
    }

    #[test]
    fn opencode_parser_prefers_substantive_final_text_over_write_content() {
        let ndjson = r#"{"type":"tool_use","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"/tmp/scratch.txt","content":"scratch"}}}}
{"type":"text","part":{"type":"text","text":"{\"output\":\"final\"}"}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"final"}"#);
    }
}
