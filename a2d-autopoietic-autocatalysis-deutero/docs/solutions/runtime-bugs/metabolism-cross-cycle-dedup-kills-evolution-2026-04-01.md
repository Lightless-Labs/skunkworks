---
title: "Metabolism Cross-Cycle Dedup Prevents Multi-Cycle Evolution"
date: 2026-04-01
category: runtime-bugs
module: metabolism
problem_type: runtime_error
component: tooling
symptoms:
  - "Cycles 2+ report 0 invocations and 0 mutations despite enzyme readiness"
  - "Only the first cycle produces any catalytic activity"
  - "Multi-cycle evolution runs complete silently with no errors but no progress"
root_cause: logic_error
resolution_type: code_fix
severity: high
tags:
  - metabolism
  - dedup
  - multi-cycle
  - evolution
  - last-inputs
  - enzyme-scheduling
---

# Metabolism Cross-Cycle Dedup Prevents Multi-Cycle Evolution

## Problem

The metabolism's per-invocation deduplication mechanism — which tracks the artifact revision each enzyme last consumed — was scoped to the metabolism's lifetime rather than to individual cycles. This meant that after cycle 1 recorded every enzyme's input revisions into `last_inputs`, cycles 2+ saw identical revisions and skipped all enzymes, making multi-cycle evolution impossible.

## Symptoms

- Cycles 2+ report 0 invocations and 0 mutations despite enzymes being ready and artifacts present
- Only the first cycle produces any catalytic activity
- Multi-cycle runs complete without errors but with no evolutionary progress beyond cycle 1

## What Didn't Work

- Investigating enzyme readiness logic — enzymes were correctly registered and their input artifacts existed
- Checking provider dispatch — providers were never called because scheduling itself was short-circuited by the stale dedup state

## Solution

Clear `last_inputs` at the start of each `run_cycle()` call. This is a one-line fix in `crates/a2d-core/src/metabolism.rs`:

```rust
pub fn run_cycle(&mut self) -> CycleReport {
    // Clear per-cycle dedup so enzymes re-run on each new cycle
    self.last_inputs.clear();
    // ...
}
```

Within a single cycle, the dedup remains correct: after an enzyme is invoked, its input revisions are recorded in `last_inputs`, preventing the same enzyme from firing again on unchanged inputs during the same cycle's fixpoint loop. But across cycles, the slate must be wiped so that enzymes can re-evaluate potentially mutated artifacts.

## Why This Works

The `last_inputs` map (`BTreeMap<EnzymeId, BTreeMap<ArtifactType, usize>>`) records, for each enzyme, the revision of every artifact it consumed on its last invocation. The scheduling logic in `schedule_ready()` compares current artifact revisions against this map and skips enzymes whose inputs have not changed.

The bug: `last_inputs` lived on the `Metabolism` struct and was never cleared between cycles. After cycle 1 ran all enzymes, cycle 2's scheduler saw identical revisions (artifacts had not been externally modified between cycles) and concluded every enzyme was already up-to-date.

The fix scopes the dedup to a single cycle by clearing at cycle start. This preserves the within-cycle invariant (no infinite loops from enzymes repeatedly firing on the same inputs) while allowing across-cycle re-evaluation, which is essential because mutations from cycle N should feed into cycle N+1's catalytic activity.

## Prevention

- When introducing memoization or dedup caches, explicitly document and enforce their intended lifetime scope (per-call, per-cycle, per-session)
- Add a multi-cycle integration test that asserts cycles 2+ produce nonzero invocations when mutations occur in cycle 1
- Treat "silent zero activity" in later cycles as a first-class failure signal, not a normal quiescent state

## Related Issues

- The `schedule_ready()` method at line ~190 of `crates/a2d-core/src/metabolism.rs` contains the dedup check against `last_inputs`
- The `last_inputs` field is defined at line 80 of the same file
