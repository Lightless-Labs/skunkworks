---
title: "Keep GLM Off the Evolver Critical Path"
date: 2026-05-22
module: provider-registry
tags:
  - provider-assignment
  - evolver
  - glm
  - latency
problem_type: runtime-bug
---

# Keep GLM Off the Evolver Critical Path

## Problem

After the scheduler was fixed to route mechanical `fitness_report` directly into the evolver, the feedback metabolism correctly prioritized:

```text
evolver → architect → tester → coder retry
```

That exposed the next bottleneck: the default live registry still assigned the evolver to GLM 5.1. In live validation, seed cycle 2 reached the evolver first and then spent the bounded feedback window on a GLM timeout. The architecture had stopped starving the evolver with tester/coder work, but the provider assignment now let the evolver starve the rest of the feedback loop.

## Fix

The live provider registry now assigns:

- coder: `opencode/kimi-for-coding/k2p6`;
- coder portfolio fallback: `opencode/opencode/deepseek-v4-flash-free`;
- evolver: `opencode/kimi-for-coding/k2p6`;
- tester/architect: `opencode/zai-coding-plan/glm-5.1`.

This keeps GLM out of both critical model-production paths: initial code production and first feedback-metabolism adaptation. The evolver assignment is explicit even though Kimi is also the default provider.

After per-invocation lineage diagnostics were added, a rerun showed a second-order leak: once Kimi and DeepSeek were cooled down by timeouts, the generic circuit breaker could still route the evolver to GLM as a fallback. The provider registry now has role-isolated fallback selection, and non-parallel evolver invocations use it. If all role-local providers are unavailable, the evolver fails explicitly on its assigned provider instead of silently consuming the tester/architect GLM window.

## Validation

Unit coverage:

- `live_registry_keeps_glm_off_coder_and_evolver_critical_path`
- `role_isolated_provider_skips_other_role_assignments`
- `role_isolated_provider_falls_back_to_assigned_instead_of_other_role`
- `only_evolver_uses_role_isolated_nonparallel_fallback`

Full test suite:

```text
cargo test
154 passed, 2 ignored
```

Bounded topology comparison:

```bash
A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=300 \
  cargo run -p a2d -- compare-topologies sudoku 3
```

Log: `/tmp/a2d-topology-compare-sudoku3-evolver-kimi-20260522.log`

Result:

| Topology | Best fitness | Wall-clock | Invocations | Failures |
|----------|--------------|------------|-------------|----------|
| seed | 83% (5/6) | 337.0s | 5 | 2 |
| evolved | 67% (4/6) | 270.4s | 4 | 2 |

The GLM evolver assignment bottleneck was removed from the default registry, but the evolved 7-enzyme topology still underperformed the seed topology in this run. This strengthens the current suspicion that the evolved topology adds orchestration overhead without reliable outcome benefit.

A follow-up run after topology lineage details exposed the fallback leak:

```text
seed cycle 2: [evolver via opencode/kimi-for-coding/k2p6] FAIL: timeout
seed cycle 3: [evolver via opencode/zai-coding-plan/glm-5.1] FAIL: timeout
```

Log: `/tmp/a2d-topology-compare-sudoku3-lineage-details-20260522.log`

Role-isolated evolver fallback was added after this run.

Live validation after the fix:

```bash
A2D_PROVIDER_TIMEOUT_SECS=90 A2D_MAX_CYCLE_SECS=300 \
  cargo run -p a2d -- compare-topologies sudoku 3
```

Log: `/tmp/a2d-topology-compare-sudoku3-role-isolated-evolver-20260522.log`

Result:

| Topology | Best fitness | Wall-clock | Invocations | Failures |
|----------|--------------|------------|-------------|----------|
| seed | 50% (3/6) | 335.2s | 6 | 2 |
| evolved | 83% (5/6) | 272.0s | 4 | 2 |

Evolver invocations stayed on Kimi k2.6 in both topologies:

```text
seed cycle 2: [evolver via opencode/kimi-for-coding/k2p6] OK
seed cycle 3: [evolver via opencode/kimi-for-coding/k2p6] OK
evolved cycle 3: [evolver via opencode/kimi-for-coding/k2p6] OK
```

The GLM fallback leak is validated fixed for this run. Remaining slow-path failures were architect/tester windows, not evolver fallback.

## Follow-up

- Address architect/tester provider latency: GLM architect timed out, then tester fallback to Kimi also timed out in the validation run.
- Re-run comparison with a true cancellable coder portfolio or with slow providers excluded from scoped waits.
- Run repeated bounded seed-vs-evolved comparisons or isolate lineage-added decomposition enzymes to distinguish topology value from provider randomness.
