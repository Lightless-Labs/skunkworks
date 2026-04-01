//! Membrane policy engine — the soft boundary of A².
//!
//! The membrane controls what tools a workcell can use, what network
//! endpoints it can reach, and what secrets it can access. The soft
//! membrane is germline-mutable (the system can evolve its own policies).
//! The hard shell is externally anchored and immutable from inside.

use a2_core::error::{A2Error, A2Result};
use a2_core::id::WorkcellId;
use a2_core::protocol::{BoundaryPolicy, CapabilityMap, NetworkPolicy};
use a2_core::traits::Membrane;

/// Default membrane implementation backed by a BoundaryPolicy.
pub struct PolicyMembrane {
    policy: BoundaryPolicy,
}

impl PolicyMembrane {
    pub fn new(policy: BoundaryPolicy) -> Self {
        Self { policy }
    }

    /// Create a fully permissive membrane for Stage 0 bootstrapping.
    pub fn permissive() -> Self {
        Self {
            policy: BoundaryPolicy {
                hard_shell: a2_core::protocol::HardShell {
                    root_of_trust_hash: "bootstrap".into(),
                    constitutional_spec_hash: "bootstrap".into(),
                    frozen_sentinel_hash: "bootstrap".into(),
                    max_budget: a2_core::protocol::Budget {
                        max_tokens: 1_000_000,
                        max_duration_secs: 3600,
                        max_calls: 1000,
                    },
                },
                soft_membrane: CapabilityMap {
                    allowed_tools: vec!["*".into()],
                    denied_tools: vec![],
                    secret_scopes: vec![],
                    network_policy: NetworkPolicy::Open,
                },
            },
        }
    }

    fn is_tool_allowed(&self, tool_name: &str) -> bool {
        let cap = &self.policy.soft_membrane;

        // Explicit deny takes precedence.
        if cap.denied_tools.iter().any(|d| d == tool_name || d == "*") {
            return false;
        }

        // Wildcard allow or explicit allow.
        cap.allowed_tools.iter().any(|a| a == "*" || a == tool_name)
    }

    fn is_endpoint_allowed(&self, endpoint: &str) -> bool {
        match &self.policy.soft_membrane.network_policy {
            NetworkPolicy::Open => true,
            NetworkPolicy::Isolated => false,
            NetworkPolicy::AllowList(allowed) => allowed.iter().any(|a| endpoint.starts_with(a)),
        }
    }
}

impl Membrane for PolicyMembrane {
    fn check_tool(&self, tool_name: &str, workcell: &WorkcellId) -> A2Result<()> {
        if self.is_tool_allowed(tool_name) {
            Ok(())
        } else {
            Err(A2Error::MembraneDenied(format!(
                "tool '{tool_name}' denied for workcell {workcell}"
            )))
        }
    }

    fn check_network(&self, endpoint: &str, workcell: &WorkcellId) -> A2Result<()> {
        if self.is_endpoint_allowed(endpoint) {
            Ok(())
        } else {
            Err(A2Error::MembraneDenied(format!(
                "endpoint '{endpoint}' denied for workcell {workcell}"
            )))
        }
    }

    fn capability_map(&self, _workcell: &WorkcellId) -> CapabilityMap {
        self.policy.soft_membrane.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2_core::id::WorkcellId;

    #[test]
    fn permissive_allows_everything() {
        let m = PolicyMembrane::permissive();
        let wc = WorkcellId::new();
        assert!(m.check_tool("git", &wc).is_ok());
        assert!(m.check_tool("rm", &wc).is_ok());
        assert!(m.check_network("https://api.openai.com", &wc).is_ok());
    }

    #[test]
    fn deny_overrides_allow() {
        let m = PolicyMembrane::new(BoundaryPolicy {
            hard_shell: a2_core::protocol::HardShell {
                root_of_trust_hash: "test".into(),
                constitutional_spec_hash: "test".into(),
                frozen_sentinel_hash: "test".into(),
                max_budget: a2_core::protocol::Budget {
                    max_tokens: 10_000,
                    max_duration_secs: 60,
                    max_calls: 10,
                },
            },
            soft_membrane: CapabilityMap {
                allowed_tools: vec!["*".into()],
                denied_tools: vec!["rm".into(), "sudo".into()],
                secret_scopes: vec![],
                network_policy: NetworkPolicy::Isolated,
            },
        });
        let wc = WorkcellId::new();

        assert!(m.check_tool("git", &wc).is_ok());
        assert!(m.check_tool("cargo", &wc).is_ok());
        assert!(m.check_tool("rm", &wc).is_err());
        assert!(m.check_tool("sudo", &wc).is_err());
        assert!(m.check_network("https://anything.com", &wc).is_err());
    }

    #[test]
    fn allowlist_network() {
        let m = PolicyMembrane::new(BoundaryPolicy {
            hard_shell: a2_core::protocol::HardShell {
                root_of_trust_hash: "test".into(),
                constitutional_spec_hash: "test".into(),
                frozen_sentinel_hash: "test".into(),
                max_budget: a2_core::protocol::Budget {
                    max_tokens: 10_000,
                    max_duration_secs: 60,
                    max_calls: 10,
                },
            },
            soft_membrane: CapabilityMap {
                allowed_tools: vec![],
                denied_tools: vec![],
                secret_scopes: vec![],
                network_policy: NetworkPolicy::AllowList(vec![
                    "https://api.anthropic.com".into(),
                    "https://api.openai.com".into(),
                ]),
            },
        });
        let wc = WorkcellId::new();

        assert!(
            m.check_network("https://api.anthropic.com/v1/messages", &wc)
                .is_ok()
        );
        assert!(
            m.check_network("https://api.openai.com/v1/chat", &wc)
                .is_ok()
        );
        assert!(m.check_network("https://evil.com", &wc).is_err());
    }
}
