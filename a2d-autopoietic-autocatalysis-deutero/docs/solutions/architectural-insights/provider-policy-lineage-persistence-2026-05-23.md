---
title: "Provider Policy Needs Lineage Persistence"
date: 2026-05-23
module: lineage
tags:
  - autopoiesis
  - provider-policy
  - lineage
  - persistence
problem_type: architectural-insight
---

# Provider Policy Needs Lineage Persistence

## Problem

The `provider_policy` artifact made provider routing typed and mechanically gated, but accepted changes were still only active in memory for the current run. A subsequent run rebuilt the hardcoded live `ProviderRegistry`, losing any accepted provider-role adaptation.

That left an incomplete loop:

```text
provider_health_report → provider_policy proposal → mechanical gate → active registry
```

The missing edge was durable lineage:

```text
active registry → provider-policy.json → next run registry
```

## Fix

The lineage archive now persists provider policy beside the germline:

- `germline.json` — enzyme topology/prompt lineage;
- `provider-policy.json` — provider-role assignment lineage.

New lineage APIs:

- `LineageArchive::commit_provider_policy(&ProviderPolicy, &CycleReport)`;
- `LineageArchive::read_provider_policy()`.

The CLI runtime now:

1. builds the hardcoded live registry;
2. loads `provider-policy.json` if present;
3. reapplies the policy through the same mechanical gate used for model-proposed policy artifacts;
4. commits accepted provider-policy changes when the cycle did not regress.

Seed mode remains isolated: `A2D_GERMLINE=seed` bypasses lineage provider policy so baseline comparisons can still use the hardcoded provider split.

Topology comparison keeps seed on defaults and lets evolved mode load lineage provider policy, treating provider policy as part of the evolved system state.

## Why this matters

Provider adaptation is now capable of surviving process boundaries. Without this, the system could observe provider failures and even accept a policy change, but it would forget the change on restart — operator memory was still part of the loop.

This separates three layers cleanly:

1. **Observation:** `provider_health_report`.
2. **Action proposal/gate:** `provider_policy` artifact and `ProviderRegistry::apply_policy`.
3. **Durability:** lineage `provider-policy.json`.

## Validation

Unit coverage added:

- `commit_and_read_provider_policy_roundtrips`;
- `loaded_provider_policy_applies_to_registered_known_enzyme`.

Full test suite:

```text
cargo test
163 passed, 2 ignored
```

RAF smoke:

```text
cargo run -q -p a2d -- status
Loaded germline from lineage (7 enzymes)
RAF coverage: 100%
Closed: yes
```

## Remaining gap

The durability gate is still lightweight: schema/provider/enzyme validation plus non-regression. The next safety step is bounded topology-comparison gating before making provider-policy changes durable defaults.
