---
title: "Fitness Ratchet Leak: Unconditional last_fitness Update on Regression"
date: 2026-04-01
category: runtime-bugs
module: metabolism
problem_type: logic_error
component: tooling
symptoms:
  - "Cycle with 0% fitness committed to lineage despite prior cycle having 40%"
  - "Ratchet failed to hold high-water mark after a regression cycle"
  - "Cycle 2 regressed to 0% (correctly blocked), but cycle 3 at 0% was committed because last_fitness had been lowered to 0%"
root_cause: logic_error
resolution_type: code_fix
severity: high
tags:
  - fitness-ratchet
  - last-fitness
  - regression-gate
  - lineage-commit
  - high-water-mark
  - metabolism
---

# Fitness Ratchet Leak: Unconditional last_fitness Update on Regression

## Problem

The fitness ratchet in `Metabolism::run_cycle()` was supposed to prevent regressions from being committed to the lineage. The delta check (`delta < 0.0` triggers skip) was correct, but `last_fitness` was updated unconditionally — including when fitness regressed. This meant a regression cycle would lower the high-water mark, allowing a subsequent cycle at the same low fitness to pass the gate.

Concrete scenario:
- Cycle 1: fitness 40%, committed (delta = 40% - 0% = +40%). `last_fitness` set to 40%.
- Cycle 2: fitness 0%, blocked (delta = 0% - 40% = -40%). But `last_fitness` set to 0%.
- Cycle 3: fitness 0%, **committed** (delta = 0% - 0% = 0%, which satisfies `>= 0`).

The ratchet leaked because the reference point (`last_fitness`) tracked the latest measurement rather than the latest committed measurement.

## Symptoms

- A cycle with 0% fitness was committed to the lineage despite a prior cycle achieving 40%
- The fitness gate appeared to work for individual regressions but failed across consecutive low-fitness cycles
- Lineage history showed declining fitness values that should have been blocked

## Solution

Guard the `last_fitness` update behind the same delta check that gates the lineage commit. Only update when `delta >= 0.0`:

```rust
let delta = fitness_report.fitness - self.last_fitness;
// Only update last_fitness if not regressing.
// If regression, the CLI will skip the lineage commit,
// and we keep last_fitness at the committed value.
if delta >= 0.0 {
    self.last_fitness = fitness_report.fitness;
}
```

This is a one-line logical change in `crates/a2d-core/src/metabolism.rs` (line ~200), wrapping the existing unconditional assignment in a conditional block.

## Why This Works

The `last_fitness` field serves as the high-water mark for the ratchet. The lineage commit gate already checks `delta < 0.0` to skip regressions, but the gate is only meaningful if the reference point it compares against reflects the last *committed* state. By making the update conditional on non-regression, `last_fitness` stays at the committed value (40% in the example) until a cycle meets or exceeds it, ensuring the ratchet holds monotonically.

The key invariant: `last_fitness` always equals the fitness of the most recent committed cycle, never the most recent measured cycle.

## Prevention

- When implementing ratchets or monotonic gates, ensure the reference value and the gate condition are updated under the same predicate — never update the reference unconditionally while gating the action conditionally
- Add a multi-cycle regression test: cycle at X%, regress to 0%, attempt 0% again — assert the third cycle is also blocked
- Treat any state mutation in a "skip" path as a code smell: if a cycle is blocked, it should be side-effect-free with respect to the ratchet state

## Related Issues

- Fix commit: `ab51180` ("Fix fitness ratchet: only update last_fitness on improvement")
- The `last_fitness` field is defined at line 89 of `crates/a2d-core/src/metabolism.rs`
- The fitness gate logic starts at line ~195 of the same file
- Related: `b0ece57` ("Add fitness-gated lineage: skip commit on regression") introduced the original gate
