use a2_core::error::{A2Error, A2Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
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
        .map_err(|e| A2Error::ProviderError(format!("Failed to execute which: {}", e)))?;

    if !output.status.success() {
        return Err(A2Error::ProviderError(format!(
            "Binary '{}' not found in PATH",
            binary
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v.get("type").and_then(|t| t.as_str()) == Some("turn.completed") {
                if let Some(usage) = v.get("usage") {
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
        }
    }
    (0, 0)
}

/// Parse token usage from Claude stream-json — looks for result event with usage.
fn parse_claude_usage(jsonl: &str) -> (u64, u64) {
    for line in jsonl.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v.get("type").and_then(|t| t.as_str()) == Some("result") {
                if let Some(usage) = v.get("usage") {
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
        }
    }
    (0, 0)
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
            .map_err(|e| A2Error::ProviderError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(A2Error::ProviderError(format!("Claude failed: {}", stderr)));
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
                .map_err(|e| A2Error::ProviderError(e.to_string()))?;
            cmd.env("GEMINI_SYSTEM_MD", &path);
            temp_sys_file = Some(path);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(e.to_string()))?;

        if let Some(path) = temp_sys_file {
            let _ = fs::remove_file(path).await;
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(A2Error::ProviderError(format!("Gemini failed: {}", stderr)));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let v: Value = serde_json::from_str(&stdout_str)
            .map_err(|e| A2Error::ProviderError(format!("Failed to parse JSON: {}", e)))?;

        let text = extract_text_recursive(&v, "text").unwrap_or_default();

        Ok(GenerateResponse {
            text,
            tokens_in: 0,
            tokens_out: 0,
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
            .map_err(|e| A2Error::ProviderError(e.to_string()))?;

        if !output.status.success() {
            let _ = fs::remove_file(&out_path).await;
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(A2Error::ProviderError(format!("Codex failed: {}", stderr)));
        }

        let text = fs::read_to_string(&out_path).await.unwrap_or_default();
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
        cmd.arg(&combined_prompt);
        cmd.arg("--format").arg("json");
        cmd.stdin(Stdio::null());

        let output = cmd
            .output()
            .await
            .map_err(|e| A2Error::ProviderError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(A2Error::ProviderError(format!(
                "OpenCode failed: {}",
                stderr
            )));
        }

        let mut full_text = String::new();
        let stdout_str = String::from_utf8_lossy(&output.stdout);

        for line in stdout_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if v.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                        full_text.push_str(text);
                    }
                }
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
            A2Error::ProviderError(format!("Provider not found: {}/{}", provider_id, model_id))
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
}
