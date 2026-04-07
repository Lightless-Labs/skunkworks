---
title: Replace stagnation strings with a StrategyChange enum for actionable adaptation
date: 2026-04-05
module: a2-governor
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - A control loop reacts to a detector that previously returned a freeform string
  - You want the reaction to be exhaustively typed and unit-testable
  - The reaction needs to dispatch into provider rotation, retries, or task decomposition
tags:
  - rust
  - enum
  - control-loop
  - adaptation
  - stagnation
---

# Replace stagnation strings with a StrategyChange enum for actionable adaptation

## Context

`StagnationDetector` originally returned a human-readable string ("things are flat, consider…"). The run loop logged it and moved on — no behavior change. The string was load-bearing for nothing.

Replacing it with a typed enum made the response actionable and exhaustive:

```rust
pub enum StrategyChange {
    None,
    SwitchModel,
    DecomposeTask,
    RaiseTemperature,
}
```

The detector now derives the variant from a small decision table over (trend, total_promotions):

| trend    | promotions | StrategyChange    |
|----------|------------|-------------------|
| Positive | any        | None              |
| Flat     | < threshold| RaiseTemperature  |
| Negative | < threshold| SwitchModel       |
| any      | 0 over N   | DecomposeTask     |

The run loop matches on the variant and dispatches concrete actions: rotating providers, bumping temperature, asking the planner to split a task.

## Guidance

When a "what should we do next" signal exists in a control loop:

1. **Never return a String for a decision.** Strings are for humans; control loops need variants.
2. **Make the enum exhaustive.** `match` without a wildcard arm forces every new variant to be handled at every dispatch site.
3. **Keep the variants action-shaped, not state-shaped.** `SwitchModel` is an action; `Stagnant` is a state. Actions compose with dispatch tables; states require interpretation.
4. **Unit-test the decision function in isolation** — pure trend → variant mapping is trivial to test and prevents regressions when thresholds move.

## Why This Matters

The string version was technically working — the system observed stagnation. But observation without action is theater. The enum forced the question "what do we actually do?" and the answer (provider rotation) turned a passive log line into a feedback loop that recovers from bad model selection automatically.

## Related

- `crates/a2-stagnation-detector/src/lib.rs`
- `crates/a2-governor/src/lib.rs` — dispatch site
- DESIGN.md — autopoietic adaptation requirements
