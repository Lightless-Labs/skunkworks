use a2_core::error::{A2Error, A2Result};
use a2_core::protocol::NetworkPolicy;
use a2_workcell::sandbox_profile::{
    materialize_provider_command_for_network_policy, sandbox_exec_available_on_this_platform,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse>;

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        if matches!(
            network_policy,
            Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))
        ) {
            return Err(A2Error::ProviderError(format!(
                "{} provider launch refused because restricted network policy requires an audited sandbox/provider allowlist launch path",
                self.provider_id()
            )));
        }
        self.generate(prompt, system).await
    }

    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
}

// --- Utilities ---

async fn resolve_binary(binary: &str) -> A2Result<String> {
    let output = Command::new("which")
        .arg(binary)
        .output()
        .await
        .map_err(|e| {
            A2Error::ProviderError(format!(
                "failed to resolve provider binary `{binary}` via `which`: {e}"
            ))
        })?;

    if !output.status.success() {
        return Err(A2Error::ProviderError(format!(
            "Binary '{}' not found in PATH",
            binary
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

const A2_PROVIDER_NETWORK_POLICY_ENV: &str = "A2_PROVIDER_NETWORK_POLICY";

fn parse_provider_network_policy(policy: Option<&str>) -> A2Result<Option<NetworkPolicy>> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    let trimmed = policy.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("open") {
        return Ok(None);
    }
    if trimmed.eq_ignore_ascii_case("isolated") {
        return Ok(Some(NetworkPolicy::Isolated));
    }

    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("allowlist:")
        .or_else(|| lower.strip_prefix("allow-list:"))
    {
        let prefix_len = trimmed.len() - rest.len();
        let endpoints: Vec<String> = trimmed[prefix_len..]
            .split(',')
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if endpoints.is_empty() {
            return Err(A2Error::ProviderError(format!(
                "{A2_PROVIDER_NETWORK_POLICY_ENV}={policy} is invalid: allowlist requires at least one endpoint"
            )));
        }
        return Ok(Some(NetworkPolicy::AllowList(endpoints)));
    }

    Err(A2Error::ProviderError(format!(
        "{A2_PROVIDER_NETWORK_POLICY_ENV}={policy} is invalid: expected open, isolated, or allowlist:<endpoint>[,<endpoint>...]"
    )))
}

fn broker_sandbox_profile_dir() -> PathBuf {
    env::temp_dir().join("a2-broker-sandbox-profiles")
}

fn materialize_broker_provider_command_for_policy(
    provider: &str,
    binary_path: &str,
    provider_args: Vec<OsString>,
    policy_value: Option<&str>,
    sandbox_exec_available: bool,
    profile_dir: &Path,
) -> A2Result<(OsString, Vec<OsString>)> {
    let policy = parse_provider_network_policy(policy_value)?;
    if policy.is_some() && !sandbox_exec_available {
        return Err(A2Error::ProviderError(format!(
            "{provider} provider launch refused because {A2_PROVIDER_NETWORK_POLICY_ENV}={}; /usr/bin/sandbox-exec is unavailable or not executable on this platform",
            policy_value.unwrap_or("")
        )));
    }
    let materialized = materialize_provider_command_for_network_policy(
        policy.as_ref(),
        profile_dir,
        OsString::from(binary_path),
        provider_args,
    )
    .map_err(|error| {
        A2Error::ProviderError(format!(
            "{provider} sandbox command materialization: {error}"
        ))
    })?;
    Ok((materialized.program, materialized.args))
}

fn materialize_broker_provider_command_with_network_policy(
    provider: &str,
    binary_path: &str,
    provider_args: Vec<OsString>,
    network_policy: &NetworkPolicy,
) -> A2Result<(OsString, Vec<OsString>)> {
    if matches!(
        network_policy,
        NetworkPolicy::Isolated | NetworkPolicy::AllowList(_)
    ) && !sandbox_exec_available_on_this_platform()
    {
        return Err(A2Error::ProviderError(format!(
            "{provider} provider launch refused because restricted network policy requires /usr/bin/sandbox-exec, which is unavailable or not executable on this platform"
        )));
    }
    let materialized = materialize_provider_command_for_network_policy(
        Some(network_policy),
        &broker_sandbox_profile_dir(),
        OsString::from(binary_path),
        provider_args,
    )
    .map_err(|error| {
        A2Error::ProviderError(format!(
            "{provider} sandbox command materialization: {error}"
        ))
    })?;
    Ok((materialized.program, materialized.args))
}

fn materialize_broker_provider_command(
    provider: &str,
    binary_path: &str,
    provider_args: Vec<OsString>,
) -> A2Result<(OsString, Vec<OsString>)> {
    let policy_value = env::var(A2_PROVIDER_NETWORK_POLICY_ENV).ok();
    materialize_broker_provider_command_for_policy(
        provider,
        binary_path,
        provider_args,
        policy_value.as_deref(),
        sandbox_exec_available_on_this_platform(),
        &broker_sandbox_profile_dir(),
    )
}

fn broker_provider_command(
    provider: &str,
    binary_path: &str,
    provider_args: Vec<OsString>,
) -> A2Result<Command> {
    let (program, args) =
        materialize_broker_provider_command(provider, binary_path, provider_args)?;
    let mut cmd = Command::new(program);
    clear_env(&mut cmd);
    cmd.args(args);
    Ok(cmd)
}

fn broker_provider_command_with_network_policy(
    provider: &str,
    binary_path: &str,
    provider_args: Vec<OsString>,
    network_policy: Option<&NetworkPolicy>,
) -> A2Result<Command> {
    let Some(network_policy) = network_policy else {
        return broker_provider_command(provider, binary_path, provider_args);
    };
    let (program, args) = materialize_broker_provider_command_with_network_policy(
        provider,
        binary_path,
        provider_args,
        network_policy,
    )?;
    let mut cmd = Command::new(program);
    clear_env(&mut cmd);
    cmd.args(args);
    Ok(cmd)
}

fn provider_launch_error(provider: &str, binary_path: &str, error: &std::io::Error) -> A2Error {
    A2Error::ProviderError(format!(
        "{provider} provider failed to launch binary `{binary_path}`: {error}"
    ))
}

fn provider_temp_file_error(
    provider: &str,
    action: &str,
    path: &Path,
    error: &std::io::Error,
) -> A2Error {
    A2Error::ProviderError(format!(
        "{provider} provider failed to {action} temp file `{}`: {error}",
        path.display()
    ))
}

fn provider_exit_error(
    provider: &str,
    model_id: &str,
    status: std::process::ExitStatus,
    stderr: &[u8],
) -> A2Error {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();

    if stderr.is_empty() {
        A2Error::ProviderError(format!(
            "{provider} provider model `{model_id}` exited with status {status} and produced no stderr"
        ))
    } else {
        A2Error::ProviderError(format!(
            "{provider} provider model `{model_id}` exited with status {status}: {stderr}"
        ))
    }
}

fn provider_parse_error(
    provider: &str,
    model_id: &str,
    format_name: &str,
    error: &serde_json::Error,
) -> A2Error {
    A2Error::ProviderError(format!(
        "{provider} provider model `{model_id}` returned invalid {format_name}: {error}"
    ))
}

fn clear_env(cmd: &mut Command) {
    cmd.env_clear();
    for var in &[
        "PATH",
        "HOME",
        "TMPDIR",
        // Claude
        "ANTHROPIC_API_KEY",
        "CLAUDE_CODE_OAUTH_TOKEN",
        // Gemini
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        // OpenAI / Codex
        "OPENAI_API_KEY",
        // ZAI / Pi
        "ZAI_API_KEY",
        "PI_CODING_AGENT_DIR",
        "PI_CODING_AGENT_SESSION_DIR",
        "PI_PACKAGE_DIR",
        "PI_OFFLINE",
        // XDG (needed by some CLI tools)
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
    ] {
        if let Ok(val) = env::var(var) {
            cmd.env(var, val);
        }
    }
    for (key, val) in env::vars() {
        if key.starts_with("OPENCODE_") || key.starts_with("PI_") {
            cmd.env(key, val);
        }
    }
}

fn extract_text_recursive(val: &Value, key: &str) -> Option<String> {
    match val {
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get(key) {
                return Some(s.clone());
            }
            for v in map.values() {
                if let Some(s) = extract_text_recursive(v, key) {
                    return Some(s);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(s) = extract_text_recursive(v, key) {
                    return Some(s);
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse token usage from Codex JSONL — looks for turn.completed with usage field.
fn parse_codex_usage(jsonl: &str) -> (u64, u64) {
    for line in jsonl.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line)
            && v.get("type").and_then(|t| t.as_str()) == Some("turn.completed")
            && let Some(usage) = v.get("usage")
        {
            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            return (input, output);
        }
    }
    (0, 0)
}

/// Parse token usage from Claude stream-json — looks for result event with usage.
fn parse_claude_usage(jsonl: &str) -> (u64, u64) {
    for line in jsonl.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line)
            && v.get("type").and_then(|t| t.as_str()) == Some("result")
            && let Some(usage) = v.get("usage")
        {
            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            return (input, output);
        }
    }
    (0, 0)
}

/// Parse token usage from Gemini JSON output.
///
/// `--output-format json` includes a `stats` object whose `models` entries
/// contain aggregated token counts. Prefer prompt/candidate totals so usage
/// aligns with the provider-level token accounting used elsewhere.
fn parse_pi_jsonl(jsonl: &str) -> (String, u64, u64) {
    let mut text = String::new();
    let mut tokens_in = 0;
    let mut tokens_out = 0;

    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
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

fn parse_gemini_usage(json: &Value) -> (u64, u64) {
    let Some(stats) = json.get("stats") else {
        return (0, 0);
    };

    if let Some(models) = stats.get("models").and_then(|v| v.as_object()) {
        let mut tokens_in = 0;
        let mut tokens_out = 0;

        for metrics in models.values() {
            let Some(tokens) = metrics.get("tokens") else {
                continue;
            };

            tokens_in += tokens
                .get("prompt")
                .or_else(|| tokens.get("input"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            tokens_out += tokens
                .get("candidates")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        }

        return (tokens_in, tokens_out);
    }

    let tokens_in = stats
        .get("input_tokens")
        .or_else(|| stats.get("input"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tokens_out = stats
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    (tokens_in, tokens_out)
}

// --- Providers ---

pub struct ClaudeProvider {
    model_id: String,
    binary_path: String,
}

impl ClaudeProvider {
    pub async fn new(model_id: &str) -> A2Result<Self> {
        let binary_path = resolve_binary("claude").await?;
        Ok(Self {
            model_id: model_id.to_string(),
            binary_path,
        })
    }
}

#[async_trait]
impl ModelProvider for ClaudeProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.generate_with_network_policy(prompt, system, None)
            .await
    }

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        let mut provider_args = vec![
            OsString::from("-p"),
            OsString::from(prompt),
            OsString::from("--model"),
            OsString::from(&self.model_id),
            OsString::from("--output-format"),
            OsString::from("stream-json"),
            OsString::from("--verbose"),
        ];

        if let Some(sys) = system {
            provider_args.push(OsString::from("--append-system-prompt"));
            provider_args.push(OsString::from(sys));
        }

        let mut cmd = broker_provider_command_with_network_policy(
            "claude",
            &self.binary_path,
            provider_args,
            network_policy,
        )?;

        let output = cmd
            .output()
            .await
            .map_err(|e| provider_launch_error("claude", &self.binary_path, &e))?;

        if !output.status.success() {
            return Err(provider_exit_error(
                "claude",
                &self.model_id,
                output.status,
                &output.stderr,
            ));
        }

        let mut full_text = String::new();
        let stdout_str = String::from_utf8_lossy(&output.stdout);

        for line in stdout_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if let Some(delta) = v.get("delta") {
                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                        full_text.push_str(text);
                    }
                } else if let Some(text) = extract_text_recursive(&v, "text") {
                    full_text.push_str(&text);
                }
            }
        }

        let (tokens_in, tokens_out) = parse_claude_usage(&stdout_str);

        Ok(GenerateResponse {
            text: full_text,
            tokens_in,
            tokens_out,
        })
    }

    fn provider_id(&self) -> &str {
        "claude"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

pub struct GeminiProvider {
    model_id: String,
    binary_path: String,
}

impl GeminiProvider {
    pub async fn new(model_id: &str) -> A2Result<Self> {
        let binary_path = resolve_binary("gemini").await?;
        Ok(Self {
            model_id: model_id.to_string(),
            binary_path,
        })
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.generate_with_network_policy(prompt, system, None)
            .await
    }

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        let provider_args = vec![
            OsString::from("-p"),
            OsString::from(prompt),
            OsString::from("--sandbox"),
            OsString::from("-o"),
            OsString::from("json"),
        ];
        let mut cmd = broker_provider_command_with_network_policy(
            "gemini",
            &self.binary_path,
            provider_args,
            network_policy,
        )?;
        cmd.stdin(Stdio::null());

        let mut temp_sys_file = None;
        if let Some(sys) = system {
            let path = env::temp_dir().join(format!(
                "gemini_sys_{}_{}.md",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros()
            ));
            fs::write(&path, sys)
                .await
                .map_err(|e| provider_temp_file_error("gemini", "write", &path, &e))?;
            cmd.env("GEMINI_SYSTEM_MD", &path);
            temp_sys_file = Some(path);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| provider_launch_error("gemini", &self.binary_path, &e))?;

        if let Some(path) = temp_sys_file {
            let _ = fs::remove_file(path).await;
        }

        if !output.status.success() {
            return Err(provider_exit_error(
                "gemini",
                &self.model_id,
                output.status,
                &output.stderr,
            ));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let v: Value = serde_json::from_str(&stdout_str)
            .map_err(|e| provider_parse_error("gemini", &self.model_id, "JSON", &e))?;

        let text = extract_text_recursive(&v, "text").unwrap_or_default();
        let (tokens_in, tokens_out) = parse_gemini_usage(&v);

        Ok(GenerateResponse {
            text,
            tokens_in,
            tokens_out,
        })
    }

    fn provider_id(&self) -> &str {
        "gemini"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

pub struct CodexProvider {
    model_id: String,
    binary_path: String,
}

impl CodexProvider {
    pub async fn new(model_id: &str) -> A2Result<Self> {
        let binary_path = resolve_binary("codex").await?;
        Ok(Self {
            model_id: model_id.to_string(),
            binary_path,
        })
    }
}

#[async_trait]
impl ModelProvider for CodexProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.generate_with_network_policy(prompt, system, None)
            .await
    }

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        let combined_prompt = if let Some(sys) = system {
            format!("{}\n\n{}", sys, prompt)
        } else {
            prompt.to_string()
        };

        let out_path = env::temp_dir().join(format!(
            "codex_out_{}_{}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        ));

        let provider_args = vec![
            OsString::from("exec"),
            OsString::from(&combined_prompt),
            OsString::from("-m"),
            OsString::from(&self.model_id),
            OsString::from("-c"),
            OsString::from("model_reasoning_effort=\"high\""),
            OsString::from("--full-auto"),
            OsString::from("--skip-git-repo-check"),
            OsString::from("--json"),
            OsString::from("-o"),
            out_path.clone().into_os_string(),
        ];
        let mut cmd = broker_provider_command_with_network_policy(
            "codex",
            &self.binary_path,
            provider_args,
            network_policy,
        )?;

        let output = cmd
            .output()
            .await
            .map_err(|e| provider_launch_error("codex", &self.binary_path, &e))?;

        if !output.status.success() {
            let _ = fs::remove_file(&out_path).await;
            return Err(provider_exit_error(
                "codex",
                &self.model_id,
                output.status,
                &output.stderr,
            ));
        }

        let text = fs::read_to_string(&out_path)
            .await
            .map_err(|e| provider_temp_file_error("codex", "read", &out_path, &e))?;
        let _ = fs::remove_file(&out_path).await;

        // Parse token usage from JSONL stdout
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let (tokens_in, tokens_out) = parse_codex_usage(&stdout_str);

        Ok(GenerateResponse {
            text,
            tokens_in,
            tokens_out,
        })
    }

    fn provider_id(&self) -> &str {
        "codex"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

pub struct PiProvider {
    model_id: String,
    binary_path: String,
}

impl PiProvider {
    pub const DEFAULT_MODEL_ID: &'static str = "zai/glm-5.1";

    pub async fn new(model_id: &str) -> A2Result<Self> {
        let binary_path = resolve_binary("pi").await?;
        Ok(Self {
            model_id: model_id.to_string(),
            binary_path,
        })
    }
}

#[async_trait]
impl ModelProvider for PiProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.generate_with_network_policy(prompt, system, None)
            .await
    }

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        let mut provider_args = vec![
            OsString::from("--model"),
            OsString::from(&self.model_id),
            OsString::from("--no-session"),
            OsString::from("--mode"),
            OsString::from("json"),
            OsString::from("--print"),
        ];
        if let Some(sys) = system {
            provider_args.push(OsString::from("--append-system-prompt"));
            provider_args.push(OsString::from(sys));
        }
        provider_args.push(OsString::from(prompt));
        let mut cmd = broker_provider_command_with_network_policy(
            "pi",
            &self.binary_path,
            provider_args,
            network_policy,
        )?;
        cmd.stdin(Stdio::null());

        let output = cmd
            .output()
            .await
            .map_err(|e| provider_launch_error("pi", &self.binary_path, &e))?;

        if !output.status.success() {
            return Err(provider_exit_error(
                "pi",
                &self.model_id,
                output.status,
                &output.stderr,
            ));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let (text, tokens_in, tokens_out) = parse_pi_jsonl(&stdout_str);

        Ok(GenerateResponse {
            text,
            tokens_in,
            tokens_out,
        })
    }

    fn provider_id(&self) -> &str {
        "pi"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

pub struct OpenCodeProvider {
    model_id: String,
    binary_path: String,
}

impl OpenCodeProvider {
    pub const DEFAULT_MODEL_ID: &'static str = "zai-coding-plan/glm-5.1";

    pub async fn new(model_id: &str) -> A2Result<Self> {
        let binary_path = resolve_binary("opencode").await?;
        Ok(Self {
            model_id: model_id.to_string(),
            binary_path,
        })
    }
}

#[async_trait]
impl ModelProvider for OpenCodeProvider {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse> {
        self.generate_with_network_policy(prompt, system, None)
            .await
    }

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<GenerateResponse> {
        let combined_prompt = if let Some(sys) = system {
            format!("{}\n\n{}", sys, prompt)
        } else {
            prompt.to_string()
        };

        let provider_args = vec![
            OsString::from("run"),
            OsString::from("--model"),
            OsString::from(&self.model_id),
            OsString::from("--format"),
            OsString::from("json"),
            OsString::from(&combined_prompt),
        ];
        let mut cmd = broker_provider_command_with_network_policy(
            "opencode",
            &self.binary_path,
            provider_args,
            network_policy,
        )?;
        cmd.stdin(Stdio::null());

        let output = cmd
            .output()
            .await
            .map_err(|e| provider_launch_error("opencode", &self.binary_path, &e))?;

        if !output.status.success() {
            return Err(provider_exit_error(
                "opencode",
                &self.model_id,
                output.status,
                &output.stderr,
            ));
        }

        let mut full_text = String::new();
        let stdout_str = String::from_utf8_lossy(&output.stdout);

        for line in stdout_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line)
                && v.get("type").and_then(|t| t.as_str()) == Some("text")
                && let Some(text) = v.get("text").and_then(|t| t.as_str())
            {
                full_text.push_str(text);
            }
        }

        Ok(GenerateResponse {
            text: full_text,
            tokens_in: 0,
            tokens_out: 0,
        })
    }

    fn provider_id(&self) -> &str {
        "opencode"
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

// --- Router ---

#[derive(Default)]
pub struct ModelRouter {
    providers: HashMap<String, Arc<dyn ModelProvider>>,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register_provider(&mut self, provider: Arc<dyn ModelProvider>) {
        let key = format!("{}/{}", provider.provider_id(), provider.model_id());
        self.providers.insert(key, provider);
    }

    pub fn get_provider(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<Arc<dyn ModelProvider>> {
        let key = format!("{}/{}", provider_id, model_id);
        self.providers.get(&key).cloned()
    }

    pub async fn route_generate(
        &self,
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        system: Option<&str>,
    ) -> A2Result<GenerateResponse> {
        let provider = self.get_provider(provider_id, model_id).ok_or_else(|| {
            A2Error::ProviderError(format!(
                "no registered model provider for `{provider_id}/{model_id}`"
            ))
        })?;
        provider.generate(prompt, system).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        provider_id: String,
        model_id: String,
    }

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn generate(
            &self,
            prompt: &str,
            _system: Option<&str>,
        ) -> A2Result<GenerateResponse> {
            Ok(GenerateResponse {
                text: format!("Mock response to: {}", prompt),
                tokens_in: 10,
                tokens_out: 20,
            })
        }

        fn provider_id(&self) -> &str {
            &self.provider_id
        }

        fn model_id(&self) -> &str {
            &self.model_id
        }
    }

    #[test]
    fn restricted_provider_network_policy_materializes_or_fails_closed_before_launch() {
        let providers = ["claude", "gemini", "codex", "pi", "opencode"];
        let open_policies = [None, Some("Open"), Some(" open ")];
        let restricted_policies = [
            "Isolated",
            " IsoLaTeD ",
            "AllowList:https://api.openai.com",
            " ALLOWLIST:https://api.anthropic.com ",
        ];
        let profile_dir = std::env::temp_dir().join(format!(
            "a2-broker-policy-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&profile_dir);

        for provider in providers {
            for policy in open_policies {
                let (program, args) = materialize_broker_provider_command_for_policy(
                    provider,
                    provider,
                    vec![OsString::from("--arg")],
                    policy,
                    false,
                    &profile_dir,
                )
                .unwrap_or_else(|error| {
                    panic!("{provider} should allow unrestricted policy {policy:?}: {error}")
                });
                assert_eq!(program, OsString::from(provider));
                assert_eq!(args, vec![OsString::from("--arg")]);
            }

            let deny_error = materialize_broker_provider_command_for_policy(
                provider,
                provider,
                vec![OsString::from("--arg")],
                Some("deny"),
                true,
                &profile_dir,
            )
            .unwrap_err()
            .to_string();
            assert!(
                deny_error.contains(
                    "A2_PROVIDER_NETWORK_POLICY=deny is invalid: expected open, isolated, or allowlist:<endpoint>[,<endpoint>...]"
                ),
                "invalid deny policy must fail before any provider launch: {deny_error}"
            );

            for policy in restricted_policies {
                let unavailable = materialize_broker_provider_command_for_policy(
                    provider,
                    provider,
                    vec![OsString::from("--arg")],
                    Some(policy),
                    false,
                    &profile_dir,
                )
                .unwrap_err();
                let unavailable_message = unavailable.to_string();
                assert!(
                    unavailable_message.contains(&format!("{provider} provider launch refused")),
                    "unexpected unavailable-runtime error for {provider}/{policy}: {unavailable_message}"
                );
                assert!(
                    unavailable_message.contains("/usr/bin/sandbox-exec is unavailable"),
                    "restricted-policy error must fail closed before unsandboxed launch: {unavailable_message}"
                );

                let (program, args) = materialize_broker_provider_command_for_policy(
                    provider,
                    provider,
                    vec![OsString::from("--arg")],
                    Some(policy),
                    true,
                    &profile_dir,
                )
                .unwrap_or_else(|error| {
                    panic!("{provider}/{policy} should materialize sandbox argv: {error}")
                });
                assert_eq!(program, OsString::from("/usr/bin/sandbox-exec"));
                assert_eq!(args[0], OsString::from("-f"));
                assert_eq!(args[2], OsString::from(provider));
                assert_eq!(args[3], OsString::from("--arg"));
            }
        }

        let _ = std::fs::remove_dir_all(&profile_dir);
    }

    #[tokio::test]
    async fn test_model_router() {
        let mut router = ModelRouter::new();
        let mock1 = Arc::new(MockProvider {
            provider_id: "mock".to_string(),
            model_id: "v1".to_string(),
        });

        router.register_provider(mock1.clone());

        let provider = router
            .get_provider("mock", "v1")
            .expect("Provider not found");
        let resp = provider.generate("hello", None).await.unwrap();
        assert_eq!(resp.text, "Mock response to: hello");
        assert_eq!(resp.tokens_in, 10);
        assert_eq!(resp.tokens_out, 20);

        let routed_resp = router
            .route_generate("mock", "v1", "world", None)
            .await
            .unwrap();
        assert_eq!(routed_resp.text, "Mock response to: world");
    }

    #[tokio::test]
    async fn test_route_generate_missing_provider_error_is_context_rich() {
        let router = ModelRouter::new();

        let error = router
            .route_generate("missing", "v1", "world", None)
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "model provider error: no registered model provider for `missing/v1`"
        );
    }

    #[test]
    fn pi_provider_uses_pi_model_ids_without_requiring_harness_binary() {
        let provider = PiProvider {
            model_id: PiProvider::DEFAULT_MODEL_ID.to_string(),
            binary_path: "pi".to_string(),
        };

        assert_eq!(provider.provider_id(), "pi");
        assert_eq!(provider.model_id(), "zai/glm-5.1");
    }

    #[test]
    fn test_parse_pi_jsonl_extracts_final_text_and_usage() {
        let jsonl = r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"draft"}],"usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4}}}
{"type":"turn_end","message":{"role":"assistant","content":[{"type":"text","text":"final"}],"usage":{"input":10,"output":20,"cacheRead":30,"cacheWrite":40}}}
"#;

        let (text, tokens_in, tokens_out) = parse_pi_jsonl(jsonl);

        assert_eq!(text, "final");
        assert_eq!(tokens_in, 80);
        assert_eq!(tokens_out, 20);
    }

    #[test]
    fn test_parse_gemini_usage_from_model_stats() {
        let v = serde_json::json!({
            "response": "hello",
            "stats": {
                "models": {
                    "gemini-3.1-pro": {
                        "tokens": {
                            "input": 80,
                            "prompt": 100,
                            "candidates": 25,
                            "cached": 20
                        }
                    },
                    "gemini-3.1-flash": {
                        "tokens": {
                            "prompt": 40,
                            "candidates": 10
                        }
                    }
                }
            }
        });

        let (tokens_in, tokens_out) = parse_gemini_usage(&v);

        assert_eq!(tokens_in, 140);
        assert_eq!(tokens_out, 35);
    }

    #[test]
    fn test_parse_gemini_usage_from_flat_stats() {
        let v = serde_json::json!({
            "response": "hello",
            "stats": {
                "input_tokens": 12,
                "output_tokens": 34
            }
        });

        let (tokens_in, tokens_out) = parse_gemini_usage(&v);

        assert_eq!(tokens_in, 12);
        assert_eq!(tokens_out, 34);
    }
}
