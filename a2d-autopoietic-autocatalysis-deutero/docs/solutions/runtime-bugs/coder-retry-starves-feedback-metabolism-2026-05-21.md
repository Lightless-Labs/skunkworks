---
title: "Coder Retry Must Not Starve Feedback Metabolism"
date: 2026-05-21
category: runtime-bugs
module: metabolism
problem_type: scheduler
component: cycle-orchestration
symptoms:
  - "After a successful coder invocation, the same cycle scheduled coder again before tester/evolver/architect could metabolize feedback"
  - "Stale ready batches allowed auxiliary decomposition to run after code had already been produced"
  - "Topology comparison spent provider time retrying coder instead of propagating benchmark feedback"
root_cause: static_code_first_priority_and_stale_ready_batch_continuation
resolution_type: code_fix
severity: high
tags:
  - scheduler
  - feedback-loop
  - coder
  - metabolism
---

# Coder Retry Must Not Starve Feedback Metabolism

## Problem

A²D's scheduler prioritized `code` producers first to prevent speculative decomposition from starving coder. That fixed one starvation mode but created another: once coder produced a code artifact and benchmark feedback, coder could immediately become ready again because `failure_report` / `fitness_report` changed.

In live topology comparison, this caused the same cycle to spend another full coder provider timeout before tester/evolver/architect could consume feedback.

This violates the project priority:

```text
learning/adapting/self-improving > quality/correctness > speed
```

A produced code artifact should become food for the feedback metabolism, not trigger an immediate same-cycle retry loop.

## Solution

Two scheduler changes landed:

1. **Successful code production advances the cycle.** After a coder invocation materializes code and benchmark fitness is recorded, the current cycle ends so the next cycle can schedule against fresh feedback state.
2. **Scheduler priority is dynamic.** Coder is highest priority only before any code exists. Once code exists, tester/evolver/architect are allowed to metabolize feedback before coder retries.

This preserves the earlier fix — coder still beats speculative decomposition before code exists — while preventing coder from starving downstream learning.

## Validation

Unit coverage added:

- `successful_coder_advances_cycle_before_stale_auxiliary_work`
- `tester_precedes_coder_once_code_exists`

Live validation:

```bash
A2D_TRACE=1 A2D_PROVIDER_TIMEOUT_SECS=180 A2D_MAX_CYCLE_SECS=300 \
  cargo run -p a2d -- compare-topologies sudoku 1
```

Observed:

- seed cycle 1: one coder invocation, 83% (5/6), then `code artifact produced; advancing cycle so feedback can metabolize`;
- evolved cycle 1: one coder invocation, 67% (4/6), then cycle advanced;
- no same-cycle coder retry;
- stale `analyze_requirements` did not run after coder success.

Log: `/tmp/a2d-topology-compare-sudoku1-cycleadvance-20260521.log`.
