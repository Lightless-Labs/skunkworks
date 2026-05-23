//! LLM Provider trait: the interface through which enzymes invoke models.
//!
//! Multi-model by design. Each enzyme can be backed by a different provider.
//! The provider trait is intentionally minimal — it takes a prompt and returns
//! a response with a trace of tool events for the observer.
//!
//! Design rationale:
//! - Multi-model diversity reduces methodological monoculture (Sawdust finding)
//! - Provider selection is per-enzyme, not per-system
//! - Every invocation produces a tool event trace for structural observation

use crate::observer::ToolEvent;
use crate::types::EnzymeId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("model invocation failed: {0}")]
    InvocationFailed(String),

    #[error("model not available: {0}")]
    ModelUnavailable(String),

    #[error("context limit exceeded: {prompt_tokens} prompt + {max_tokens} max > {limit} limit")]
    ContextLimitExceeded {
        prompt_tokens: usize,
        max_tokens: usize,
        limit: usize,
    },
}

/// A model invocation request.
#[derive(Debug, Clone)]
pub struct InvocationRequest {
    /// Which enzyme is making this request.
    pub enzyme_id: EnzymeId,
    /// The system prompt (enzyme definition / role).
    pub system: String,
    /// The user prompt (task description + inputs).
    pub prompt: String,
    /// Maximum tokens to generate.
    pub max_tokens: usize,
}

/// A model invocation response.
#[derive(Debug, Clone)]
pub struct InvocationResponse {
    /// The generated text output after provider-specific parsing.
    pub text: String,
    /// Raw provider stdout/body when available. This is not routed as an
    /// artifact; it exists so empty/invalid parsed-output failures can include
    /// a sanitized preview of what the provider actually emitted.
    pub raw_output: Option<String>,
    /// Tool events observed during generation (for the structural observer).
    /// Populated by the provider from the model's tool use trace.
    pub tool_events: Vec<ToolEvent>,
    /// Thinking block content, if available. The 93% of reasoning
    /// that's normally suppressed — the richest signal source.
    pub thinking: Option<String>,
    /// Token usage for cost tracking.
    pub usage: TokenUsage,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub thinking_tokens: usize,
}

impl TokenUsage {
    pub fn total(&self) -> usize {
        self.prompt_tokens + self.completion_tokens + self.thinking_tokens
    }
}

/// The provider trait. Implementations wrap specific model APIs.
///
/// Each provider is stateless — it doesn't maintain conversation history.
/// State flows through the workcell context and germline, not through
/// the provider. This enforces ephemeral sessions by construction.
pub trait Provider: Send + Sync {
    /// Human-readable name of this provider (e.g., "claude-opus-4-6", "codex").
    fn name(&self) -> &str;

    /// Invoke the model with a request.
    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError>;

    /// Whether this provider supports thinking block access.
    /// If true, InvocationResponse.thinking will be populated.
    fn supports_thinking(&self) -> bool {
        false
    }
}

/// Typed provider-role assignment policy.
///
/// This is the serializable mechanism form of provider routing: a model may
/// propose it as a `provider_policy` artifact, but the registry only accepts
/// assignments that target known enzymes and registered provider names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPolicy {
    #[serde(default)]
    pub assignments: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAssignmentChange {
    pub enzyme_id: EnzymeId,
    pub previous_provider: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPolicyRejection {
    pub enzyme_id: Option<EnzymeId>,
    pub provider: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderPolicyApplication {
    pub accepted: Vec<ProviderAssignmentChange>,
    pub rejected: Vec<ProviderPolicyRejection>,
}

/// Provider registry: maps enzyme IDs to their assigned providers.
///
/// This is where multi-model assignment happens. Different enzymes
/// can use different models based on their strengths:
/// - Evolver: Claude (best at policy, decomposition, review)
/// - Coder: Codex (strongest executor for repo surgery)
/// - Tester: Gemini (long-context over traces)
/// - Cheap mutations: OpenCode backends (low-cost batch experimentation)
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
    assignments: std::collections::BTreeMap<EnzymeId, usize>,
    default_provider: usize,
}

impl ProviderRegistry {
    pub fn new(default_provider: Box<dyn Provider>) -> Self {
        Self {
            providers: vec![default_provider],
            assignments: std::collections::BTreeMap::new(),
            default_provider: 0,
        }
    }

    /// Register a new provider, returns its index.
    pub fn register(&mut self, provider: Box<dyn Provider>) -> usize {
        let idx = self.providers.len();
        self.providers.push(provider);
        idx
    }

    /// Assign a provider to an enzyme.
    pub fn assign(&mut self, enzyme_id: EnzymeId, provider_idx: usize) {
        self.assignments.insert(enzyme_id, provider_idx);
    }

    /// Current typed provider policy. Unassigned enzymes use the registry
    /// default and therefore do not appear in the assignment map.
    pub fn current_policy(&self) -> ProviderPolicy {
        let assignments = self
            .assignments
            .iter()
            .map(|(enzyme_id, idx)| (enzyme_id.0.clone(), self.providers[*idx].name().to_string()))
            .collect();
        ProviderPolicy { assignments }
    }

    /// Apply a typed provider policy after mechanical validation.
    pub fn apply_policy(
        &mut self,
        policy: &ProviderPolicy,
        valid_enzyme_ids: &BTreeSet<EnzymeId>,
    ) -> ProviderPolicyApplication {
        let mut application = ProviderPolicyApplication::default();

        for (enzyme, provider_name) in &policy.assignments {
            let enzyme_id = EnzymeId(enzyme.clone());
            if !valid_enzyme_ids.contains(&enzyme_id) {
                application.rejected.push(ProviderPolicyRejection {
                    enzyme_id: Some(enzyme_id),
                    provider: Some(provider_name.clone()),
                    reason: "target enzyme is not in the current germline".to_string(),
                });
                continue;
            }

            let Some(provider_idx) = self.provider_index_by_name(provider_name) else {
                application.rejected.push(ProviderPolicyRejection {
                    enzyme_id: Some(enzyme_id),
                    provider: Some(provider_name.clone()),
                    reason: "provider is not registered".to_string(),
                });
                continue;
            };

            let previous_provider = self.provider_for(&enzyme_id).name().to_string();
            self.assign(enzyme_id.clone(), provider_idx);
            if previous_provider != *provider_name {
                application.accepted.push(ProviderAssignmentChange {
                    enzyme_id,
                    previous_provider,
                    provider: provider_name.clone(),
                });
            }
        }

        application
    }

    fn provider_index_by_name(&self, provider_name: &str) -> Option<usize> {
        self.providers
            .iter()
            .position(|provider| provider.name() == provider_name)
    }

    /// Get the provider for an enzyme (falls back to default).
    pub fn provider_for(&self, enzyme_id: &EnzymeId) -> &dyn Provider {
        let idx = self
            .assignments
            .get(enzyme_id)
            .copied()
            .unwrap_or(self.default_provider);
        self.providers[idx].as_ref()
    }

    /// Get a provider DIFFERENT from the one assigned to this enzyme.
    /// Used by escalation rung 2 to consult an alternative model.
    /// Returns the default provider if it differs from the assigned one,
    /// otherwise returns the next registered provider.
    pub fn alternative_provider_for(&self, enzyme_id: &EnzymeId) -> &dyn Provider {
        let assigned_idx = self.assigned_index(enzyme_id);
        self.alternative_provider_excluding_index(assigned_idx)
    }

    /// Get an alternative provider unless that alternative is temporarily
    /// unavailable. Used for consultations where the assigned provider should
    /// not be the first choice, but known-unhealthy providers should still be
    /// avoided.
    pub fn alternative_provider_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> &dyn Provider {
        let assigned_idx = self.assigned_index(enzyme_id);
        let alternative = self.alternative_provider_excluding_index(assigned_idx);
        if !unavailable_provider_names.contains(alternative.name()) {
            return alternative;
        }

        for (idx, provider) in self.providers.iter().enumerate() {
            if idx != assigned_idx && !unavailable_provider_names.contains(provider.name()) {
                return provider.as_ref();
            }
        }

        self.providers[assigned_idx].as_ref()
    }

    /// Get the provider for an enzyme unless that provider is temporarily
    /// unavailable. If the assigned provider is unavailable, choose a healthy
    /// alternative. If every provider is unavailable, fall back to the assigned
    /// provider so single-provider deployments still make progress/fail loudly.
    pub fn provider_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> &dyn Provider {
        self.providers_for_avoiding(enzyme_id, unavailable_provider_names)
            .into_iter()
            .next()
            .expect("registry always has a provider")
    }

    /// Ordered provider candidates for an enzyme, excluding temporarily
    /// unavailable providers when possible. The assigned provider comes first,
    /// followed by every other healthy provider. If all providers are marked
    /// unavailable, return the assigned provider so the failure is explicit.
    pub fn providers_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> Vec<&dyn Provider> {
        let assigned_idx = self.assigned_index(enzyme_id);
        let mut candidates = Vec::new();

        if !unavailable_provider_names.contains(self.providers[assigned_idx].name()) {
            candidates.push(self.providers[assigned_idx].as_ref());
        }

        for (idx, provider) in self.providers.iter().enumerate() {
            if idx != assigned_idx && !unavailable_provider_names.contains(provider.name()) {
                candidates.push(provider.as_ref());
            }
        }

        if candidates.is_empty() {
            candidates.push(self.providers[assigned_idx].as_ref());
        }

        candidates
    }

    /// Get a provider for an enzyme while preserving role isolation. Unlike
    /// `provider_for_avoiding`, this excludes providers explicitly assigned to
    /// other enzymes. If every role-local provider is unavailable, fall back to
    /// the assigned provider so the failure is explicit instead of silently
    /// consuming another role's provider window.
    pub fn role_isolated_provider_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> &dyn Provider {
        self.role_isolated_providers_for_avoiding(enzyme_id, unavailable_provider_names)
            .into_iter()
            .next()
            .expect("registry always has a provider")
    }

    /// Ordered candidates for an enzyme while preserving role isolation.
    /// Unlike `providers_for_avoiding`, this excludes providers explicitly
    /// assigned to other enzymes; a tester/architect provider should not
    /// consume its role-specific session budget as a speculative coder or
    /// fallback evolver.
    pub fn role_isolated_providers_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> Vec<&dyn Provider> {
        let assigned_idx = self.assigned_index(enzyme_id);
        let assigned_elsewhere = self
            .assignments
            .iter()
            .filter(|(assigned_enzyme, _)| *assigned_enzyme != enzyme_id)
            .map(|(_, idx)| *idx)
            .collect::<std::collections::BTreeSet<_>>();

        let mut candidates = Vec::new();
        if !unavailable_provider_names.contains(self.providers[assigned_idx].name()) {
            candidates.push(self.providers[assigned_idx].as_ref());
        }

        for (idx, provider) in self.providers.iter().enumerate() {
            if idx != assigned_idx
                && !assigned_elsewhere.contains(&idx)
                && !unavailable_provider_names.contains(provider.name())
            {
                candidates.push(provider.as_ref());
            }
        }

        if candidates.is_empty() {
            candidates.push(self.providers[assigned_idx].as_ref());
        }

        candidates
    }

    /// Ordered candidates for parallel invocation of one enzyme. This is the
    /// role-isolated provider set used concurrently by the coder portfolio.
    pub fn parallel_providers_for_avoiding(
        &self,
        enzyme_id: &EnzymeId,
        unavailable_provider_names: &std::collections::BTreeSet<String>,
    ) -> Vec<&dyn Provider> {
        self.role_isolated_providers_for_avoiding(enzyme_id, unavailable_provider_names)
    }

    fn assigned_index(&self, enzyme_id: &EnzymeId) -> usize {
        self.assignments
            .get(enzyme_id)
            .copied()
            .unwrap_or(self.default_provider)
    }

    fn alternative_provider_excluding_index(&self, assigned_idx: usize) -> &dyn Provider {
        // Prefer the default provider if it's different from the assigned one.
        if assigned_idx != self.default_provider {
            return self.providers[self.default_provider].as_ref();
        }

        // Otherwise, pick the first registered provider that isn't the assigned one.
        for (idx, provider) in self.providers.iter().enumerate() {
            if idx != assigned_idx {
                return provider.as_ref();
            }
        }

        // Fallback: only one provider registered. Return it anyway.
        self.providers[self.default_provider].as_ref()
    }

    /// List all registered providers.
    pub fn providers(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.name()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock provider for testing.
    struct MockProvider {
        name: String,
        response: String,
        thinking: Option<String>,
    }

    impl MockProvider {
        fn new(name: &str, response: &str) -> Self {
            Self {
                name: name.to_string(),
                response: response.to_string(),
                thinking: None,
            }
        }

        fn with_thinking(mut self, thinking: &str) -> Self {
            self.thinking = Some(thinking.to_string());
            self
        }
    }

    impl Provider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn invoke(
            &self,
            _request: &InvocationRequest,
        ) -> Result<InvocationResponse, ProviderError> {
            Ok(InvocationResponse {
                text: self.response.clone(),
                raw_output: None,
                tool_events: vec![ToolEvent::Read, ToolEvent::Think, ToolEvent::Execute],
                thinking: self.thinking.clone(),
                usage: TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    thinking_tokens: if self.thinking.is_some() { 200 } else { 0 },
                },
            })
        }

        fn supports_thinking(&self) -> bool {
            self.thinking.is_some()
        }
    }

    #[test]
    fn registry_routes_to_assigned_provider() {
        let mut registry =
            ProviderRegistry::new(Box::new(MockProvider::new("default", "default response")));

        let claude_idx =
            registry.register(Box::new(MockProvider::new("claude", "claude response")));
        let codex_idx = registry.register(Box::new(MockProvider::new("codex", "codex response")));

        registry.assign(EnzymeId::from("evolver"), claude_idx);
        registry.assign(EnzymeId::from("coder"), codex_idx);

        assert_eq!(
            registry.provider_for(&EnzymeId::from("evolver")).name(),
            "claude"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("coder")).name(),
            "codex"
        );
        // Unassigned enzyme falls back to default
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "default"
        );
    }

    #[test]
    fn provider_invocation_returns_tool_events() {
        let provider = MockProvider::new("test", "output");
        let request = InvocationRequest {
            enzyme_id: EnzymeId::from("coder"),
            system: "You are a coder.".into(),
            prompt: "Write hello world.".into(),
            max_tokens: 1000,
        };

        let response = provider.invoke(&request).unwrap();
        assert_eq!(response.text, "output");
        assert_eq!(response.tool_events.len(), 3);
        assert!(response.thinking.is_none());
    }

    #[test]
    fn thinking_block_access_when_supported() {
        let provider = MockProvider::new("claude", "output")
            .with_thinking("I should consider the edge cases...");

        assert!(provider.supports_thinking());

        let request = InvocationRequest {
            enzyme_id: EnzymeId::from("evolver"),
            system: "You are an evolver.".into(),
            prompt: "Improve the coder enzyme.".into(),
            max_tokens: 2000,
        };

        let response = provider.invoke(&request).unwrap();
        assert!(response.thinking.is_some());
        assert!(response.thinking.unwrap().contains("edge cases"));
        assert_eq!(response.usage.thinking_tokens, 200);
    }

    #[test]
    fn token_usage_totals_correctly() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            thinking_tokens: 200,
        };
        assert_eq!(usage.total(), 350);
    }

    #[test]
    fn alternative_provider_returns_different_from_assigned() {
        let mut registry =
            ProviderRegistry::new(Box::new(MockProvider::new("default", "default response")));
        let claude_idx =
            registry.register(Box::new(MockProvider::new("claude", "claude response")));
        let _codex_idx = registry.register(Box::new(MockProvider::new("codex", "codex response")));

        // Assigned to claude → alternative should be default (not claude)
        registry.assign(EnzymeId::from("evolver"), claude_idx);
        let alt = registry.alternative_provider_for(&EnzymeId::from("evolver"));
        assert_eq!(alt.name(), "default");
        assert_ne!(alt.name(), "claude");

        // Assigned to default (via fallback) → alternative should be first non-default
        let alt2 = registry.alternative_provider_for(&EnzymeId::from("unassigned"));
        assert_ne!(alt2.name(), "default");
        assert_eq!(alt2.name(), "claude"); // first registered after default

        // Explicitly assigned to default → alternative should be first non-default
        registry.assign(EnzymeId::from("tester"), 0);
        let alt3 = registry.alternative_provider_for(&EnzymeId::from("tester"));
        assert_eq!(alt3.name(), "claude");
    }

    #[test]
    fn alternative_provider_single_provider_returns_itself() {
        let registry = ProviderRegistry::new(Box::new(MockProvider::new("only", "only response")));
        let alt = registry.alternative_provider_for(&EnzymeId::from("anything"));
        assert_eq!(alt.name(), "only");
    }

    #[test]
    fn provider_for_avoiding_skips_unavailable_assigned_provider() {
        let mut registry =
            ProviderRegistry::new(Box::new(MockProvider::new("default", "default response")));
        let gemini_idx =
            registry.register(Box::new(MockProvider::new("gemini", "gemini response")));
        registry.assign(EnzymeId::from("tester"), gemini_idx);

        let mut unavailable = std::collections::BTreeSet::new();
        unavailable.insert("gemini".to_string());

        let provider = registry.provider_for_avoiding(&EnzymeId::from("tester"), &unavailable);

        assert_eq!(provider.name(), "default");
    }

    #[test]
    fn role_isolated_provider_skips_other_role_assignments() {
        let mut registry =
            ProviderRegistry::new(Box::new(MockProvider::new("kimi", "kimi response")));
        let deepseek =
            registry.register(Box::new(MockProvider::new("deepseek", "deepseek response")));
        let glm = registry.register(Box::new(MockProvider::new("glm", "glm response")));

        registry.assign(EnzymeId::from("evolver"), 0);
        registry.assign(EnzymeId::from("tester"), glm);
        registry.assign(EnzymeId::from("architect"), glm);

        let mut unavailable = std::collections::BTreeSet::new();
        unavailable.insert("kimi".to_string());

        let provider =
            registry.role_isolated_provider_for_avoiding(&EnzymeId::from("evolver"), &unavailable);

        assert_eq!(provider.name(), "deepseek");
        assert_eq!(deepseek, 1);
    }

    #[test]
    fn role_isolated_provider_falls_back_to_assigned_instead_of_other_role() {
        let mut registry =
            ProviderRegistry::new(Box::new(MockProvider::new("kimi", "kimi response")));
        registry.register(Box::new(MockProvider::new("deepseek", "deepseek response")));
        let glm = registry.register(Box::new(MockProvider::new("glm", "glm response")));

        registry.assign(EnzymeId::from("evolver"), 0);
        registry.assign(EnzymeId::from("architect"), glm);

        let mut unavailable = std::collections::BTreeSet::new();
        unavailable.insert("kimi".to_string());
        unavailable.insert("deepseek".to_string());

        let provider =
            registry.role_isolated_provider_for_avoiding(&EnzymeId::from("evolver"), &unavailable);

        assert_eq!(provider.name(), "kimi");
    }

    #[test]
    fn provider_policy_applies_registered_provider_to_known_enzyme() {
        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", "")));
        registry.register(Box::new(MockProvider::new("fast", "")));

        let mut policy = ProviderPolicy::default();
        policy
            .assignments
            .insert("coder".to_string(), "fast".to_string());

        let valid = BTreeSet::from([EnzymeId::from("coder")]);
        let application = registry.apply_policy(&policy, &valid);

        assert_eq!(application.accepted.len(), 1);
        assert!(application.rejected.is_empty());
        assert_eq!(application.accepted[0].previous_provider, "default");
        assert_eq!(application.accepted[0].provider, "fast");
        assert_eq!(
            registry.provider_for(&EnzymeId::from("coder")).name(),
            "fast"
        );
        assert_eq!(
            registry.current_policy().assignments.get("coder"),
            Some(&"fast".to_string())
        );
    }

    #[test]
    fn provider_policy_rejects_unknown_provider_and_unknown_enzyme() {
        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", "")));

        let mut policy = ProviderPolicy::default();
        policy
            .assignments
            .insert("coder".to_string(), "missing".to_string());
        policy
            .assignments
            .insert("ghost".to_string(), "default".to_string());

        let valid = BTreeSet::from([EnzymeId::from("coder")]);
        let application = registry.apply_policy(&policy, &valid);

        assert!(application.accepted.is_empty());
        assert_eq!(application.rejected.len(), 2);
        assert_eq!(
            registry.provider_for(&EnzymeId::from("coder")).name(),
            "default"
        );
    }

    #[test]
    fn registry_lists_all_providers() {
        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", "")));
        registry.register(Box::new(MockProvider::new("claude", "")));
        registry.register(Box::new(MockProvider::new("codex", "")));

        let names = registry.providers();
        assert_eq!(names, vec!["default", "claude", "codex"]);
    }
}
