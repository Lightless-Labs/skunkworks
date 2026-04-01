use a2_core::error::{A2Error, A2Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::Path;
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
        // XDG (needed by some CLI tools)
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
    ] {
        if let Ok(val) = env::var(var) {
            cmd.env(var, val);
        }
    }
    for (key, val) in env::vars() {
        if key.starts_with("OPENCODE_") {
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
        let mut cmd = Command::new(&self.binary_path);
        clear_env(&mut cmd);

        cmd.arg("-p")
            .arg(prompt)
            .arg("--model")
            .arg(&self.model_id)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");

        if let Some(sys) = system {
            cmd.arg("--append-system-prompt").arg(sys);
        }

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
        let mut cmd = Command::new(&self.binary_path);
        clear_env(&mut cmd);

        cmd.arg("-p").arg(prompt);
        cmd.arg("--sandbox");
        cmd.arg("-o").arg("json");
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
        let mut cmd = Command::new(&self.binary_path);
        clear_env(&mut cmd);

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

        cmd.arg("exec");
        cmd.arg(&combined_prompt);
        cmd.arg("-m").arg(&self.model_id);
        cmd.arg("-c").arg("model_reasoning_effort=\"high\"");
        cmd.arg("--full-auto");
        cmd.arg("--skip-git-repo-check");
        cmd.arg("--json");
        cmd.arg("-o").arg(&out_path);

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
        let mut cmd = Command::new(&self.binary_path);
        clear_env(&mut cmd);

        let combined_prompt = if let Some(sys) = system {
            format!("{}\n\n{}", sys, prompt)
        } else {
            prompt.to_string()
        };

        cmd.arg("run");
        cmd.arg("--model").arg(&self.model_id);
        cmd.arg("--format").arg("json");
        cmd.arg(&combined_prompt);
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
