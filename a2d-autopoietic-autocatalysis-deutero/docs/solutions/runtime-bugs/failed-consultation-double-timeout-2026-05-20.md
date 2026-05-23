---
title: "Failed Consultation Must Not Spend a Second Provider Timeout"
date: 2026-05-20
category: runtime-bugs
module: metabolism
problem_type: bounded_runtime
component: escalation-ladder
symptoms:
  - "A nominally bounded cycle spent one provider timeout on rung-2 consultation and then another on the primary invocation"
  - "Topology comparison hit the outer harness timeout during evolved cycle 3"
  - "Cycle wall-clock cap is only checked between invocations, so consultation + primary could exceed the cap inside one workcell"
root_cause: failed_consultation_fell_through_to_primary_invocation
resolution_type: code_fix
severity: high
tags:
  - escalation
  - provider-timeout
  - consultation
  - bounded-benchmarks
---

# Failed Consultation Must Not Spend a Second Provider Timeout

## Problem

A live `compare-topologies sudoku 3` run with `A2D_PROVIDER_TIMEOUT_SECS=180` and `A2D_MAX_CYCLE_SECS=240` exposed a bounded-runtime leak.

At coder rung 2+, A²D asks an alternative provider for consultation before the primary invocation. When the consultation provider timed out after 180s, the metabolism still invoked the primary provider in the same workcell. That primary then consumed another 180s timeout.

Observed in `/tmp/a2d-topology-compare-sudoku3-20260520.log`:

- seed cycle 3: 180s consultation timeout + 180s primary timeout = ~360s for one cycle;
- evolved cycle 3 entered the same pattern and the outer command timed out before the comparison could finish.

This violates the spirit of the cycle wall-clock cap. The cap is checked between invocations, but consultation and primary are both inside one workcell invocation.

## Solution

A failed consultation now terminates the workcell immediately:

- record provider failure/cooldown for the consultation provider;
- mark the workcell failed with `consultation failed before primary invocation: ...`;
- return the failed `InvocationLineage` without invoking the primary provider.

A failed consultation is already a failed escalation attempt. Spending a second full timeout on the primary in the same workcell provides little value and destroys benchmark boundedness.

## Validation

Unit coverage added: `failed_consultation_does_not_invoke_primary_in_same_workcell`.

Full test suite passes: 145 tests passing, 2 ignored.

Live smoke:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=5 A2D_MAX_CYCLE_SECS=15 \
  cargo run -p a2d -- compare-topologies sudoku 3
```

Observed seed and evolved cycle 3 each spent one 5s consultation timeout and then ended the workcell immediately. No second primary timeout was spent after failed consultation.

## Related

- `todos/bounded-live-benchmarks.md`
- `docs/solutions/best-practices/topology-comparison-harness-2026-05-20.md`
- `docs/solutions/best-practices/parallel-cheap-coder-race-2026-05-19.md`
