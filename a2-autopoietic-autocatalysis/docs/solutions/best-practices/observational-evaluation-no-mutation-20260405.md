---
title: Make evaluation observational — never mutate the substrate you score against
date: 2026-04-05
module: a2ctl-benchmark
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - Designing a benchmark or fitness function for a self-modifying system
  - The candidate edit is producible as a diff and runnable in a sandbox
  - You are tempted to add an --apply flag "for convenience"
tags:
  - evaluation
  - benchmark
  - architecture
  - self-modification
  - separation-of-concerns
---

# Make evaluation observational — never mutate the substrate you score against

## Context

A²'s benchmark started life with a `--apply` flag that, after scoring a candidate, applied the diff back to the workspace. This entangled three concerns: candidate generation, scoring, and integration. Each concern grew its own bugs (apply context mismatch, residue tests outside `mod tests`, workspace cleanup, untracked file accounting). The total surface area required six helper functions just to keep the workspace consistent across runs.

Removing `--apply` deleted **219 lines** in one commit and eliminated all of the following:

- `append_benchmark_test_content`
- `assert_benchmark_workspace_clean`
- `cleanup_benchmark_workspace`
- `workspace_untracked_files`
- `parse_untracked_files`
- `run_workspace_shell_command`

Promotion is now decided exclusively by the worktree's own test results. The workspace is never touched. The benchmark became a pure function of (model, task, baseline ref) → score.

## Guidance

Design rule: **the evaluator reads, the integrator writes.** Never combine them in one tool.

Concretely, for any system that runs candidates in a sandbox:

1. The sandbox (worktree, container, VM) is the only place the candidate's effects exist.
2. The score is derived from observations inside the sandbox (test exit code, metrics, log content).
3. Promotion to the main substrate is a separate, explicit, downstream step — usually a human or a different tool.
4. If you need to "verify the patch lands," that verification belongs in the sandbox, not in the workspace.

The smell to watch for: helper functions named `cleanup_*`, `revert_*`, `assert_*_clean`. Each one is evidence that evaluation is mutating something it shouldn't.

## Why This Matters

Self-modifying systems are uniquely vulnerable to evaluation entanglement because the system is both the actor and the substrate. Every coupling between evaluation and substrate creates a feedback path the system can exploit (Fontana Level 0) or trip over (the apply context bug). Forcing observational purity is the cheapest defense.

Concretely for A²: the same benchmark that previously required careful workspace bookkeeping now runs against any base ref, can be invoked concurrently, and produces zero residue. The codebase is smaller, the semantics are clearer, and a class of bugs is gone by construction.

## Examples

Before:
```
a2ctl bench --apply --task 003 --model gemini
# mutates workspace, scores, may leave dangling tests
```

After:
```
a2ctl bench --task 003 --model gemini
# spawns worktree from bench-baseline, scores from worktree test results, exits
```

## Related

- `crates/a2ctl/src/bench.rs`
- `docs/solutions/workflow-issues/benchmark-staleness-and-apply-path-20260405.md`
- Karpathy's autoresearch — `evaluate_bpb` is frozen and exogenous, the same principle
