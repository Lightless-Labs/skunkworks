---
title: "Benchmark-driven autonomous evolution: vanity metrics vs real capability"
date: 2026-04-04
category: best-practices
module: a2-autonomous-loop
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - "Running autonomous improvement loops (A² or similar)"
  - "Evaluating whether an autonomous system is actually improving"
  - "Choosing metrics to drive selection pressure in evolutionary systems"
  - "Routing tasks to multiple AI models"
  - "Designing benchmark suites for code-generating agents"
tags:
  - autonomous-evolution
  - benchmarks
  - vanity-metrics
  - model-routing
  - selection-pressure
  - a2
  - worktree-catalyst
---

# Benchmark-driven autonomous evolution: vanity metrics vs real capability

## Context

A2 ran 14+ autonomous improvement rounds overnight that produced zero capability improvement. Test count stayed flat at 41, zero patches successfully auto-applied. The system was generating tasks, "solving" them, and promoting the results -- but the solutions never landed. Health metrics (test count, sentinel pass rate, promotion rate) all looked normal. When switched to benchmark-driven evaluation with known-good tasks and verification tests, the real score was 4/5 (Claude) and 1/5 (Gemini), exposing that most "improvements" were illusory.

The core problem: the autonomous loop had no real selection pressure. It optimized for whatever was easiest to measure (test count, promotion rate) rather than what matters (can it actually solve tasks end-to-end). (auto memory [claude]: project_a2_state.md confirms the benchmark baseline and the fibonacci path mismatch bug that contributed to apply failures)

## Guidance

### 1. Distinguish vanity metrics from capability benchmarks

Test count, sentinel pass rate, and promotion rate are **health metrics** -- they tell you the system is running, not that it is improving. The real benchmark is: give the system a task with a known-good solution and a verification test. Did it actually solve it?

A2 was promoting patches that never applied, inflating promotion rate while actual capability was flat. The stagnation detector should have fired but the metrics it watched were not capability metrics.

### 2. Use the benchmark-driven loop pattern

```
Run benchmark -> get score ->
  if score < target: generate improvement tasks targeting the failures
  if score = target: generate harder benchmarks
```

The benchmark IS the selection pressure. Without it, the system optimizes for whatever is easiest to measure. With it, every improvement round is anchored to demonstrated capability on tasks with verifiable outcomes.

### 3. Route tasks by demonstrated model capability

Claude scored 4/5, Gemini scored 1/5 on identical benchmark tasks. Models are not interchangeable. The system needs model-aware scheduling: route tasks to models based on their demonstrated benchmark scores, not a round-robin or random assignment.

| Model | Benchmark Score | Notes |
|-------|----------------|-------|
| Claude Sonnet 4.6 | 4/5 | Best quality, highest quota cost |
| Gemini 3.1 Pro | 1/5 | Patches often fail to apply |

### 4. Benchmark tasks must be self-contained

The first benchmark suite had a test that referenced a private helper function. It passed in the test module but failed when injected into a different scope. Every benchmark task must:
- Include its own verification test
- Not depend on private/internal APIs from the host crate
- Be runnable in isolation from the task definition alone

### 5. Fix the apply path (WorktreeCatalyst)

WorktreeCatalyst creates diffs relative to the workspace root, but `git apply` was running from the repo root (which has `a2-autopoietic-autocatalysis/` as a subdirectory). This caused the fibonacci benchmark task to consistently fail. The fix: run `git apply` from the workspace root, not the repo root.

## Why This Matters

Without benchmark-driven selection pressure, an autonomous system can run indefinitely while making zero progress. It will appear healthy on all internal metrics while producing no actual capability improvement. 14 wasted rounds overnight is a concrete example, but the pattern generalizes: any evolutionary system without genuine fitness evaluation will drift toward optimizing for measurement artifacts rather than real outcomes.

The benchmark-driven loop is the difference between an autonomous system that actually improves and one that just generates activity.

## When to Apply

- Any time an autonomous loop runs unattended (overnight, batch runs)
- When evaluating whether to add a new model provider to the routing pool
- When designing the feedback signal for any self-improving system
- When the stagnation detector fires but health metrics look normal -- this is the classic symptom of vanity metric optimization
- When benchmark scores diverge significantly between models on the same tasks

## Examples

**Before (vanity metric loop):**
```
Round 1: 41 tests, 6/6 sentinels, 3 tasks promoted -> "healthy"
Round 2: 41 tests, 6/6 sentinels, 2 tasks promoted -> "healthy"
...
Round 14: 41 tests, 6/6 sentinels, 1 task promoted -> "healthy"
Reality: zero patches applied, zero capability gain
```

**After (benchmark-driven loop):**
```
Benchmark: 4/5 (Claude), 1/5 (Gemini)
-> Fibonacci task failing: git apply path mismatch
-> Fix: apply from workspace root
-> Re-benchmark: score improves
-> Generate harder benchmarks when 5/5 reached
```

**Model-aware routing:**
```rust
// Route based on demonstrated capability
match model.benchmark_score() {
    score if score >= 4 => assign_complex_tasks(model),
    score if score >= 2 => assign_simple_tasks(model),
    _ => benchmark_only(model), // don't assign real work yet
}
```

## Related

- Auto memory: `project_a2_state.md` -- A2 operational state including benchmark baselines and known bugs
- A2 benchmark tasks: `a2-autopoietic-autocatalysis/bench/tasks/*.toml`
- WorktreeCatalyst: the apply mechanism that needs workspace-root-relative paths
