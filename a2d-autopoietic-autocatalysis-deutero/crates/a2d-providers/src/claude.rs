//! Claude API provider via the Anthropic Messages API.
//!
//! Supports thinking block extraction (extended thinking) — the 93%
//! of suppressed reasoning that's the richest signal for the observer.

use a2d_core::observer::ToolEvent;
use a2d_core::provider::{
    InvocationRequest, InvocationResponse, Provider, ProviderError, TokenUsage,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl ClaudeProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            base_url: "https://api.anthropic.com/v1/messages".to_string(),
        }
    }

    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    fn parse_tool_events(content: &[ContentBlock]) -> Vec<ToolEvent> {
        let mut events = Vec::new();
        for block in content {
            match block {
                ContentBlock::Thinking { .. } => events.push(ToolEvent::Think),
                ContentBlock::Text { text } => {
                    // Heuristic: if the text mentions tool use, classify accordingly
                    // In production this would parse actual tool_use blocks
                    if text.contains("Read") || text.contains("read") {
                        events.push(ToolEvent::Read);
                    } else if text.contains("Execute") || text.contains("bash") {
                        events.push(ToolEvent::Execute);
                    } else if text.contains("Write") || text.contains("write") {
                        events.push(ToolEvent::Write);
                    } else {
                        events.push(ToolEvent::Text);
                    }
                }
                ContentBlock::ToolUse { name, .. } => match name.as_str() {
                    "Read" | "Glob" | "Grep" => events.push(ToolEvent::Read),
                    "Bash" | "Execute" => events.push(ToolEvent::Execute),
                    "Write" | "Edit" => events.push(ToolEvent::Write),
                    _ => events.push(ToolEvent::Execute),
                },
            }
        }
        events
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        &self.model
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        let body = ApiRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            system: Some(request.system.clone()),
            messages: vec![Message {
                role: "user".to_string(),
                content: request.prompt.clone(),
            }],
        };

        let response = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| ProviderError::InvocationFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(ProviderError::InvocationFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let api_response: ApiResponse = response
            .json()
            .map_err(|e| ProviderError::InvocationFailed(e.to_string()))?;

        let mut text = String::new();
        let mut thinking = None;

        for block in &api_response.content {
            match block {
                ContentBlock::Text { text: t } => text.push_str(&t),
                ContentBlock::Thinking { thinking: t } => thinking = Some(t.clone()),
                ContentBlock::ToolUse { .. } => {}
            }
        }

        let tool_events = Self::parse_tool_events(&api_response.content);

        Ok(InvocationResponse {
            text,
            raw_output: None,
            tool_events,
            thinking,
            usage: TokenUsage {
                prompt_tokens: api_response.usage.input_tokens,
                completion_tokens: api_response.usage.output_tokens,
                thinking_tokens: 0, // TODO: extract from usage when available
            },
        })
    }

    fn supports_thinking(&self) -> bool {
        true
    }
}

// --- API types ---

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: usize,
    output_tokens: usize,
}
