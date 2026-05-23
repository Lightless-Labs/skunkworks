---
title: "Provider Policy Must Be a Gated Metabolic Mechanism"
date: 2026-05-22
module: metabolism
tags:
  - autopoiesis
  - provider-policy
  - provider-health
  - feedback-loop
problem_type: architectural-insight
---

# Provider Policy Must Be a Gated Metabolic Mechanism

## Problem

Provider health was made metabolic food, but provider assignment was still effectively a human-operated Rust constant. That left the system with evidence about provider degradation but no typed mechanism for changing role-provider routing.

The desired loop is:

```text
provider failures/timeouts → provider_health_report → provider_policy proposal → mechanical gate → ProviderRegistry
```

## Fix

A typed `provider_policy` artifact now exists.

Schema:

```json
{
  "assignments": {
    "coder": "opencode/kimi-for-coding/k2p6",
    "tester": "opencode/zai-coding-plan/glm-5.1"
  }
}
```

The active `ProviderRegistry` serializes this policy into the artifact graph every cycle. Any enzyme that produces `provider_policy` can propose changes, but the metabolism gates them mechanically before applying them:

- target enzyme must exist in the current germline;
- provider name must be registered in the current registry;
- malformed JSON is rejected without mutating the registry.

Accepted/rejected provider-policy changes are recorded in invocation lineage and cycle summaries. The current policy is also routed as catalyst context to evolver and architect prompts.

## Why this matters

This turns provider routing from an operator-only control surface into a metabolizable mechanism. It is still not fully autonomous durable adaptation, but it creates the typed boundary where autonomy can be safely added.

The system can now distinguish:

- provider health evidence (`provider_health_report`);
- provider assignment mechanism (`provider_policy`);
- mechanical gate outcomes (accepted/rejected policy changes).

That separation matters because provider health is observation, while provider policy is action.

## Validation

Unit coverage added:

- `provider_policy_applies_registered_provider_to_known_enzyme`
- `provider_policy_rejects_unknown_provider_and_unknown_enzyme`
- `provider_policy_artifact_is_gated_and_changes_later_routing`
- `provider_policy_rejects_malformed_unknown_or_unregistered_changes`

CLI germline normalization now keeps `provider_policy` in baseline food and as evolver/architect catalyst context.

Full test suite:

```text
cargo test
160 passed, 2 ignored
```

RAF smoke:

```text
cargo run -q -p a2d -- status
Loaded germline from lineage (7 enzymes)
RAF coverage: 100%
Closed: yes
```

## Follow-up

The next slice is durability and stronger gating:

1. persist accepted provider policy in the lineage archive beside `germline.json`;
2. add a lightweight policy-proposal enzyme only if it does not starve coder/feedback metabolism;
3. gate durable policy changes with bounded topology comparisons instead of only schema/provider validation.
