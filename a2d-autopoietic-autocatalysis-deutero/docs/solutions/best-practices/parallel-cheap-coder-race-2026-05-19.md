---
title: "Parallel Cheap Coder Race"
date: 2026-05-19
category: best-practices
module: metabolism
problem_type: architecture_change
component: provider-dispatch
symptoms:
  - "Serial provider fallback doubles wall-clock when cheap providers timeout"
  - "Critical coder path waits for GLM failure before trying Kimi"
  - "Cheap/good-enough providers are available but not exploited concurrently"
root_cause: critical_path_provider_dispatch_was_serial
resolution_type: code_fix
severity: medium
tags:
  - provider
  - parallelism
  - coder
  - cheap-models
  - dispatch
---

# Parallel Cheap Coder Race

## Problem

A²D had a circuit breaker, but fallback was serial: GLM had to timeout before Kimi could try the same coder work in a later cycle. With 60s timeouts this costs ~120s for two failed attempts; with GLM's 900s default it can consume the whole bounded run before a fallback has a chance.

For cheap/good-enough providers this is the wrong economics. If two providers are cheap and independent, run them concurrently and let the sandbox decide whether any produced artifact is good enough.

## Solution

Coder invocations now use a parallel provider race by default:

- Only enzymes producing `code` are parallelized.
- The assigned provider and unassigned fallback providers run concurrently.
- Providers explicitly assigned to other enzymes are excluded, so tester/evolver role-specific providers are not consumed as speculative coders.
- The selected response is the first candidate in provider order that materializes the expected artifact.
- Losing successful providers clear their health state; losing failed providers are cooled down without escalating the enzyme rung when another provider succeeded.
- `A2D_PARALLEL_CODER=0` disables this path for controlled experiments.

This is not consensus. It is a cheap race: produce candidate code quickly, then the existing sandbox/fitness machinery remains the selector.

## Validation

Unit coverage added: `parallel_coder_uses_fallback_success_in_same_cycle` verifies that an assigned provider failure and fallback success produce a successful coder invocation in one cycle.

`cargo test` passes: 141 tests passing, 2 ignored.

Live smoke:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=60 A2D_MAX_CYCLE_SECS=180 cargo run -p a2d -- challenge sudoku 1
```

Observed both GLM and Kimi spawned concurrently for coder. Both timed out at 60s, so best fitness remained 0%, but wall-clock was ~60s instead of serial ~120s. Log: `/tmp/a2d-sudoku1-20260519-parallel-coder-60s.log`.

## Related

- `todos/bounded-live-benchmarks.md`
- `docs/solutions/runtime-bugs/provider-circuit-breaker-temporary-cooldown-2026-04-23.md`
- `docs/solutions/best-practices/multi-model-dispatch-mechanical-selection-2026-04-01.md`
