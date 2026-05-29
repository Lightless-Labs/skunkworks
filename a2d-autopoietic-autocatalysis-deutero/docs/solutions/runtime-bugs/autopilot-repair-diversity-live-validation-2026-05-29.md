---
title: "Autopilot Repair Diversity Needs Live Fault Injection"
date: 2026-05-29
module: autopilot
tags:
  - autopilot
  - repair
  - provider-diversity
  - fault-injection
problem_type: runtime-bug
---

# Autopilot Repair Diversity Needs Live Fault Injection

## Problem

Autopilot's provider-diverse repair path was unit-covered but not live-validated. Normal successful maintainer outputs skip the repair loop, so waiting for a natural malformed patchset is an unreliable way to prove the Pi → alternate-provider transition and monitor metadata.

## Fix

Added an explicit validation harness:

```bash
A2D_AUTOPILOT_FAULT_INJECTION=attempt0_parse_failure \
A2D_PROVIDER_TIMEOUT_SECS=90 \
cargo run -q -p a2d -- autopilot --iterations 1 --repair-attempts 1
```

The fault injection is narrow and opt-in:

- only active when `A2D_AUTOPILOT_FAULT_INJECTION` is set;
- currently only supports `attempt0_parse_failure` aliases;
- only mutates attempt 0's parsed maintainer output after the provider returns;
- logs an `autopilot_fault_injected` monitor event with the provider, attempt, fault, and original output byte count.

This preserves the live provider call while deterministically forcing the mechanical parse-failure → repair prompt → alternate-provider route.

## Live validation

Run: `.a2d/autopilot/runs/run-1780061191713-0/`  
Console log: `/tmp/a2d-autopilot-repair-diversity-20260529132612.log`

Observed events:

```text
maintainer_provider_topology:
  primary_provider: pi/default
  registered_providers: [kimi k2p6, deepseek v4 flash, glm 5.1, pi/default]

maintainer_invocation_started:
  attempt: 0
  provider: pi/default
  escalated: false

autopilot_fault_injected:
  attempt: 0
  provider: pi/default
  fault: attempt0_parse_failure
  original_output_bytes: 9539

patchset_parse_failed:
  attempt: 0
  error: expected value at line 1 column 1

repair_attempt_started:
  attempt: 1
  provider: opencode/kimi-for-coding/k2p6
  primary_provider: pi/default
  escalated: true
  escalation_reason: first repair attempt uses configured alternate maintainer provider

maintainer_invocation_failed:
  attempt: 1
  provider: opencode/kimi-for-coding/k2p6
  error: opencode timed out after 90s

repair_budget_exhausted:
  attempt: 1
```

## Result

Validated:

- a live Pi maintainer invocation occurred;
- a repairable parse failure entered the repair loop;
- monitor logs recorded the primary provider topology;
- repair attempt 1 escalated to the configured alternate provider (`opencode/kimi-for-coding/k2p6`);
- failure after the bounded repair budget stopped cleanly;
- no real-tree patch was applied and no partial commit occurred.

Not yet validated:

- a successful alternate-provider repair patchset that passes parse/path/temp/real-tree gates. Kimi timed out under the 90s bound, so provider diversity is live but repair success remains dependent on provider latency/quality.

## Follow-up

The repair path should support a configurable or healthier alternate maintainer provider. Kimi k2p6 is currently the default alternate because the registry default is the coder provider; a faster explicit repair provider (for example DeepSeek v4 flash when healthy) may make successful repair validation more reliable without changing the primary Pi maintainer path.

## Addendum: configurable repair provider

Implemented after the first validation run:

- `A2D_AUTOPILOT_REPAIR_PROVIDER=<registered-provider-name>`;
- `a2d autopilot --repair-provider <registered-provider-name>`;
- `ProviderRegistry::provider_named` for exact registered-provider lookup;
- monitor logs include `configured_repair_provider` in `maintainer_provider_topology`;
- repair attempt 1 uses the configured provider when registered and different from primary, otherwise falls back to the previous alternate-provider behavior.

Validation:

```text
cargo test
33 CLI tests + 134 core tests + 11 bootstrap + 7 provider + 1 doctest = 186 passing, 2 ignored
```

Live probes with `opencode/opencode/deepseek-v4-flash-free` as the configured repair provider:

1. `run-1780062413070-0` (`A2D_PROVIDER_TIMEOUT_SECS=120`): primary `pi/default` timed out; repair attempt 1 used DeepSeek and returned a typed patchset, but it contained zero replacements and was correctly rejected by the path gate (`patchset must contain at least one replacement`).
2. `run-1780062590484-0` (`A2D_PROVIDER_TIMEOUT_SECS=300`): primary `pi/default` returned, fault injection forced parse failure, repair attempt 1 used DeepSeek, and DeepSeek timed out after 300s.

So configurability is implemented and live-observed, but a successful alternate-provider repair that passes path/temp/real-tree gates remains unproven. The remaining bottleneck is provider reliability/latency and repair prompt quality, not inability to route to a chosen repair provider.
