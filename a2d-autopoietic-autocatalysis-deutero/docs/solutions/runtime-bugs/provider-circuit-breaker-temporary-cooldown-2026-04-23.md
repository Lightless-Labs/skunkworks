---
title: "Provider Failures Need Temporary Circuit Breakers, Not Permanent Bans"
date: 2026-04-23
category: runtime-bugs
module: metabolism
problem_type: resilience_pattern
component: provider-dispatch
symptoms:
  - "Gemini quota exhaustion consumes several minutes before failing"
  - "Tester retries the same unhealthy Gemini provider immediately after architect fails"
  - "Cycle wall-clock cap prevents hangs but still wastes provider timeout windows"
root_cause: provider_health_not_modeled
resolution_type: code_fix
severity: high
tags:
  - provider-health
  - circuit-breaker
  - escalation
  - timeout
  - quota
  - bounded-runtime
  - multi-model-dispatch
---

# Provider Failures Need Temporary Circuit Breakers, Not Permanent Bans

## Problem

After architect context pyramid summaries landed, `a2d challenge sudoku 1` no longer had a prompt-size problem. The live trace showed:

- `system_code snapshot = 15 files, ~18 KB`
- architect Gemini prompt arg ~= 38 KB
- architect still spent ~284 s failing with Gemini quota exhaustion
- tester then spent 300 s timing out on the same Gemini provider

The cycle wall-clock cap stopped the run from becoming unbounded, but it did not prevent repeated attempts against a provider that was already known unhealthy in the current process.

## Key distinction

A bad artifact is not a provider outage.

- Code fails acceptance tests: route sandbox diagnostics to the coder/evolver.
- Provider times out, hits quota, cannot authenticate, or CLI exits non-zero: route around that provider temporarily.

Treating provider failures as artifact failures wastes time and misleads the enzyme. The model did not produce a wrong solution; the provider failed to provide a usable response.

## Solution

Add a lightweight provider circuit breaker in `Metabolism`:

1. Track `ProviderHealth` keyed by provider name.
2. On provider invocation failure, set `cooldown_until = now + backoff`.
3. During cooldown, `ProviderRegistry::provider_for_avoiding` routes enzymes assigned to that provider to a healthy alternative.
4. If every provider is unavailable, fall back to the assigned provider so single-provider deployments fail loudly rather than silently doing nothing.
5. On provider success, clear that provider's health record.

The cooldown is temporary by design. This avoids both bad extremes:

- **Never retry:** unacceptable; providers recover from quota windows and transient timeouts.
- **Retry immediately:** also unacceptable; it repeats a known outage and burns wall-clock budget.

The implemented first pass uses time-based cooldown rather than a full open/half-open/closed state machine. When cooldown expires, the assigned provider naturally becomes eligible again. A successful later call clears its failure count.

## Why this avoids permanent provider bans

Provider assignment remains unchanged:

```text
Architect: Gemini
Tester: Gemini
```

Runtime routing is conditional:

```text
Gemini healthy              → use Gemini
Gemini failed recently      → temporarily use another provider
Gemini cooldown expired     → Gemini eligible again
Gemini succeeds             → clear failure state
Gemini fails again          → reopen cooldown with backoff
```

So the provider is not removed from the topology. It is only bypassed during an outage window.

## Backoff policy

Current policy:

- Base cooldown: 600 s by default
- CLI override: `A2D_PROVIDER_COOLDOWN_SECS`
- Repeated failures: exponential backoff
- Maximum cooldown: 3600 s

This is intentionally simple. It captures the important behavior — no same-cycle retry storms — without introducing durable provider quarantine state.

## Interaction with existing escalation

Provider failure also increments the enzyme's escalation counter. This matters because repeated provider failures are still degradation events from the metabolism's perspective: the enzyme did not produce an artifact.

However, provider health is tracked separately from enzyme output loops. That separation preserves the right repair path:

- provider unhealthy → route to another provider
- output loops or bad fitness → alter prompt/session/model strategy

## Tests

Coverage added:

- `provider_for_avoiding_skips_unavailable_assigned_provider`
- `provider_failure_cools_down_provider_and_routes_following_enzyme_to_alternative`
- `provider_is_retried_after_zero_cooldown_expires`

The last test is the guard against accidental permanent bans: with zero cooldown, the same provider is retried on the next cycle and can recover.

## Live probe: 2026-04-28

`A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` confirmed the failure recording path against real providers:

```text
provider failure: gemini/gemini-3.1-pro-preview for architect → cooldown 600s, enzyme rung 1
```

The run did **not** reach a live reroute trace because after the Gemini architect failure, the next Kimi coder invocation timed out after 300 s and the 600 s cycle wall-clock cap fired before a later Gemini-assigned tester/architect invocation could be scheduled. Result: 3 invocations, `[wall-clock-capped]`, Fitness 83% (5/6).

Interpretation: provider cooldown recording is live-validated; provider rerouting remains unit-validated but needs a live run with a larger cycle budget or provider-specific timeout controls to observe `provider circuit breaker: routing ...` under real providers.

## Live validation: 2026-04-28 all-GLM default

After switching the default live registry to GLM 5.1 and isolating CLI provider cwd, `A2D_TRACE=1 cargo run -p a2d -- challenge sudoku 1` observed real rerouting:

```text
provider failure: opencode/zai-coding-plan/glm-5.1 for architect → cooldown 600s
provider circuit breaker: routing coder from opencode/zai-coding-plan/glm-5.1 to opencode/kimi-for-coding/k2p5
provider circuit breaker: routing tester from opencode/zai-coding-plan/glm-5.1 to opencode/kimi-for-coding/k2p5
provider circuit breaker: routing architect from opencode/zai-coding-plan/glm-5.1 to opencode/kimi-for-coding/k2p5
```

The run completed with 6 invocations, `[wall-clock-capped]`, Fitness 100% (6/6), and clean `git status`. This validates the core circuit-breaker behavior under real providers.

## Remaining work

This is a minimal circuit breaker, not the final escalation ladder:

- Add richer failure classification (quota vs timeout vs auth vs missing CLI).
- Implement full rung 4 provider/model swap with history.
- Consider half-open probes if real-time cooldown alone is too coarse.
- Persist provider health only if empirical evidence says process-local state is insufficient.

## Related

- `docs/solutions/runtime-bugs/provider-invocations-need-timeouts-and-output-format-handling-2026-04-04.md`
- `todos/bounded-live-benchmarks.md`
- `todos/escalation-rungs-4-6.md`
