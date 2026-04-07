---
module: bench
date: 2026-04-04
problem_type: best_practice
component: testing_framework
severity: high
related_components:
  - tooling
  - documentation
tags:
  - self-modifying-systems
  - evaluation-design
  - autopoiesis
  - benchmark
  - architecture
applies_when:
  - "Designing a self-modifying or self-improving software system"
  - "Building a benchmark for an autonomous coding agent"
  - "Deciding what artifacts an evaluator may produce vs mutate"
---

# Evaluation must not touch the germline

## Principle

In any self-modifying system, the evaluator and the mutator must be
architecturally separated. The evaluator observes; the mutator changes state.
Code produced by a benchmark task is an *evaluation instrument*, not a
*candidate mutation*. It must never be applied to the workspace whose evolution
is being measured.

Borrowing from biology: the evaluator operates on the soma; only the kernel's
own selection loop touches the germline. If the benchmark output flows back
into the project source, the system has no way to distinguish "we got better"
from "we measured ourselves into the answer."

## Why it matters

The intuitive (and wrong) move on a self-modifying coding system is: "we asked
the model to add function `foo`; it produced good code; let's commit it." This
collapses two distinct functions:

1. **Measurement**: did this candidate model/strategy/temperature successfully
   solve the task?
2. **Selection**: should the resulting artifact become part of the system?

Conflating them creates two failure modes:

- **Metric capture (Fontana Level 0)**: once `foo` exists, every future run of
  the same task trivially "passes" by reporting it already exists. The system
  collapses to the smallest behavior that satisfies the metric.
- **Evaluator drift**: the thing being measured and the thing doing the
  measuring co-evolve, and the score becomes meaningless. Karpathy's
  autoresearch deliberately freezes `evaluate_bpb` *outside* the mutable loop
  for exactly this reason — it is a telescope, not a cell.

## The architectural rule

Three artifacts, three locations, one direction of flow:

```
                  ┌──────────────┐
   task spec ───▶ │  workcell    │  ephemeral worktree, throwaway
                  │  (model run) │
                  └──────┬───────┘
                         │ stdout / diff
                         ▼
                  ┌──────────────┐
                  │  evaluator   │  reads only; writes scores
                  │  (frozen)    │
                  └──────┬───────┘
                         │ score
                         ▼
                  ┌──────────────┐
                  │  selection   │  may promote a *separate*
                  │  loop        │  candidate to germline
                  └──────────────┘
```

Rules:

- The benchmark workcell is destroyed after scoring. Its diffs never reach
  `main`.
- The evaluator's source lives outside the loop's reachable mutation surface,
  or is pinned to a tag the loop cannot rewrite.
- Promotion to the germline is a *different* code path, fed by the selection
  policy, not by the benchmark.

## Consequences for benchmark design

If the benchmark cannot mutate the germline, then "add function X" tasks need a
stable reference point that does not drift as the project evolves. Three
viable strategies:

1. **Pinned baseline tag** (e.g. `bench-baseline`): worktrees check out a
   fixed historical commit, so "add function X" remains a meaningful task even
   after X exists on `main`.
2. **Auto-generated tasks from current gaps**: regenerate the task set from
   the live codebase at each run, so tasks are always about features that
   genuinely don't exist yet.
3. **External benchmarks** (SWE-bench, etc.): let an outside corpus supply the
   ground truth so the system has no leverage to game it.

A² currently uses (1). All three are legitimate; what is *not* legitimate is
asking the live project "add function X" while letting the answer flow back
into the live project.

## Test for whether you have this right

Ask: *if I ran the benchmark twice in a row with no other changes, would the
second run be measurably easier than the first?* If yes, the benchmark output
is contaminating the evaluation surface. The germline has a leak.
