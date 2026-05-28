---
title: "Provider Policy Runtime Proposals Must Stay Comparison-Gated"
date: 2026-05-28
module: provider-policy
tags:
  - provider-policy
  - topology-gate
  - autopoiesis
  - lineage
problem_type: architectural-insight
---

# Provider Policy Runtime Proposals Must Stay Comparison-Gated

## Finding

A live runtime `provider_policy` proposal exercised the full autonomous path:

```text
provider emits provider_policy artifact
→ metabolism applies schema/provider/enzyme gate in memory
→ cycle summary records accepted provider-policy change
→ CLI runs current-vs-proposed comparison gate
→ lineage persistence is withheld when comparison evidence is missing
```

Validation log: `/tmp/a2d-provider-policy-runtime-proposal-20260528162751.log`.

## Validation setup

To avoid changing the live 7-enzyme topology, the lineage germline was temporarily replaced and restored after the run. The probe germline contained a `maintainer` enzyme that produced `provider_policy`; the hardcoded live registry already assigns `maintainer` to `pi/default`, so the initial proposal came from Pi's ephemeral artifact mode.

The probe emitted this policy proposal:

```json
{"assignments":{"maintainer":"opencode/kimi-for-coding/k2p6"}}
```

Run command:

```bash
A2D_PROVIDER_TIMEOUT_SECS=20 \
A2D_MAX_CYCLE_SECS=30 \
A2D_PROVIDER_POLICY_GATE_CYCLES=1 \
cargo run -q -p a2d -- cycle 1 \
  'Runtime validation: emit a provider_policy proposal for the maintainer enzyme and let the comparison gate decide durability.'
```

## Result

The runtime path accepted the policy in memory:

```text
[maintainer via pi/default]
outcome: SUCCESS
Provider policy accepted: 1
Provider policy rejected: 0
```

The durability gate then compared current and proposed provider policies:

```text
policy delta: maintainer: pi/default -> opencode/kimi-for-coding/k2p6
current policy:  best 0% (0/0), 1 invocation, 0 failures
proposed policy: best 0% (0/0), 1 invocation, 1 failure
Provider policy gate: REJECT — missing fitness evidence
⚠ Provider policy gate rejected durable commit: missing fitness evidence
```

No `.a2d/lineage/provider-policy.json` remained after the run.

## Why this matters

Schema/provider/enzyme validation is necessary but not sufficient for self-modifying provider routing. A syntactically valid provider assignment can be accepted transiently so the current run can experiment, but durability must require outcome evidence. Otherwise provider latency noise or a lucky non-regressing cycle can turn a bad role assignment into the next default.

The validated safety property is:

> Runtime provider-policy proposals may change in-memory routing, but cannot become lineage defaults unless the bounded comparison gate has usable fitness evidence and accepts the proposed policy.

## Follow-up

The probe also showed that a topology with no code-producing enzyme yields `0/0` fitness evidence, correctly forcing rejection. Future provider-policy enzymes should be evaluated only after they can coexist with coder/feedback metabolism without starving benchmark evidence.
