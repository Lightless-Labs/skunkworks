---
title: "Escalation Rung 4 Should Swap Providers Ephemerally"
date: 2026-05-31
module: metabolism
tags:
  - escalation
  - provider-routing
  - circuit-breaker
  - autonomy
problem_type: architectural-insight
---

# Escalation Rung 4 Should Swap Providers Ephemerally

## Problem

Rungs 0–3 detected repeated outputs and added stronger prompt interventions, but they still invoked the same assigned provider after loop detection. The provider circuit breaker only reacts to provider invocation failures/timeouts; it does not react to semantically valid but behaviorally stagnant outputs.

Durable `provider_policy` exists, but using lineage-persisted policy mutation as the first rung-4 mechanism would overfit transient challenge evidence and requires topology-comparison gating.

## Fix

Rung 4 now performs a non-persistent provider swap for the current invocation:

- `ProviderRegistry::swapped_provider_for_avoiding` selects a non-assigned provider while avoiding providers currently in cooldown;
- `ProviderRegistry::role_isolated_swapped_provider_for_avoiding` does the same while excluding providers assigned to other roles, preserving the existing evolver role-isolation invariant;
- `Metabolism::invoke_scheduled` uses the swapped provider when `enzyme_loop_count >= 4`;
- the provider assignment and durable `provider_policy` are not mutated;
- the swap automatically stops when the enzyme escapes the loop and its counter resets;
- rung 4 preserves `failure_report` history for the new provider, while rung 5+ remains the clean-session variant.

Rung 4 also skips the rung-2 consultation call. At rung 4 the alternative provider is the primary intervention, so consulting and then invoking the same alternate would spend two provider windows in one workcell.

## Coverage

Added mock tests proving:

- rung 4 invokes the swapped provider and does not invoke the assigned primary;
- rung 4 does not fire below threshold;
- when a swapped provider produces a fresh output signature, loop state resets and the next invocation returns to the assigned provider;
- rung-4 prompts include a provider-swap notice and preserve failure history;
- swapped provider selection does not mutate assignment state;
- role-isolated swaps exclude providers assigned to other roles.

Validation:

```text
cargo test
36 CLI tests + 140 core tests + 11 bootstrap + 7 provider + 1 doctest = 195 passing, 2 ignored
```

## Why ephemeral first

Ephemeral swap is the smallest useful rung-4 mechanism. It gives the loop a different model when behavioral stagnation is detected, without changing durable provider policy or requiring a human approval step. If the alternate provider escapes the loop, the loop counter reset returns routing to normal assignment automatically. Durable provider-policy changes remain a separate, comparison-gated mechanism.
