---
title: Benchmark staleness — tasks become useless once features are committed
date: 2026-04-05
category: workflow-issues
module: a2ctl-benchmark
problem_type: workflow_issue
component: tooling
severity: high
applies_when:
  - Benchmark tasks test for adding specific functions or features
  - The benchmark runs against HEAD which already has those features
  - Multiple models are tested across sessions with accumulated commits
  - Benchmark uses --apply to verify patches land on the workspace
tags:
  - benchmark
  - staleness
  - worktree
  - git-apply
  - self-modification
  - evaluation
---

# Benchmark staleness — tasks become useless once features are committed

## Context

A²'s benchmark suite asks models to "add function X to file Y" and verifies by running tests. This works perfectly the first time. But once a model successfully implements X and the result is committed, every subsequent benchmark run sees X already exists and reports "no changes needed" — scoring 0/N.

This is the fundamental tension in a self-modifying system's evaluation: **the evaluation function measures the system's ability to produce changes, but successful changes make the evaluation function obsolete.**

The problem manifested in three layers:

1. **Stale tasks**: All 5 original benchmark tasks (001-005) were solved and committed. Gemini scored 0/5 — not because it was broken, but because every task was already done.
2. **Apply path mismatch**: `append_benchmark_test_content()` modified the workspace before the worktree ran. The worktree diffed against clean HEAD, but `git apply` targeted the modified workspace — context lines didn't match.
3. **Baseline divergence**: Pinning worktrees to a `bench-baseline` tag fixed worktree staleness, but the workspace is always HEAD. Applying a baseline-relative patch to a HEAD workspace fails when files diverge.

## Guidance

### Layer 1: Replace stale tasks with fresh ones

When benchmark tasks become solved, replace them with tasks that test genuinely unimplemented features. Verify the test content references the correct types, field names, and signatures by reading the actual source files.

### Layer 2: Revert workspace before applying worktree patches

```rust
// Before calling try_apply_patch in the benchmark loop:
if let Err(e) = revert_workspace() {
    eprintln!("[revert before apply failed: {e}]");
}
match try_apply_patch(&patch.diff, &workspace) {
    Ok(true) => {
        // Re-append test content so verification can find it.
        if let Err(e) = append_benchmark_test_content(&bench_task) {
            eprintln!("[re-append test content failed: {e}]");
        }
        // ... verify ...
    }
}
```

### Layer 3: Pin worktrees to a baseline ref

```rust
// WorktreeCatalyst now supports branching from any git ref:
pub fn with_base_ref(workspace_root: PathBuf, base_ref: impl Into<String>) -> Self

// Benchmark uses a tag:
let catalyst = WorktreeCatalyst::with_base_ref(workspace.clone(), "bench-baseline");
```

The `bench-baseline` tag must point to the commit where benchmark task features do NOT yet exist.

### Unsolved: HEAD-vs-baseline divergence on apply

When HEAD has changes to the same files the baseline-relative patch targets, `git apply` fails on context mismatch. Current workarounds:

- Run benchmark without `--apply` for measurement only
- Keep `bench-baseline` close to HEAD (re-tag after writing new tasks)
- Accept that `--apply` in bench mode is fragile when HEAD diverges significantly

The proper fix would be: apply the patch in a temporary checkout of the baseline, verify there, then cherry-pick the result to HEAD. This hasn't been implemented yet.

## Why This Matters

This is not a minor tooling bug — it's the **autoresearch problem** that Karpathy solved by freezing `evaluate_bpb`. In A², the evaluation function (benchmark tasks) is inside the mutable loop. Successful self-modification invalidates the fitness signal. Without a solution, the system converges to "everything already exists" and the autonomous loop produces zero signal.

The deeper lesson: any self-modifying system needs evaluation that is either (a) exogenous and immutable (autoresearch pattern), (b) pinned to a stable baseline (bench-baseline pattern), or (c) auto-generated from the current state (not yet implemented). Option (c) is the autopoietic answer — the system generates its own evaluation criteria — but requires solving the Fontana Level 0 problem (the system gaming its own metrics).

## When to Apply

- After any successful benchmark run that commits results to HEAD
- When adding new benchmark tasks — verify they test genuinely unimplemented features
- When running benchmarks concurrently with manual development — expect workspace conflicts
- When designing evaluation for any self-modifying system

## Examples

**Before** (stale benchmark, Gemini 0/5):
```
Add a fibonacci function    gemini  no  no    # "already exists"
Add a Timeout error variant  gemini  no  no    # "already exists"
```

**After** (fresh tasks targeting real gaps, Gemini 5/5):
```
Add trend detection to StagnationDetector  gemini  yes  yes
Add deny_tool method to PolicyMembrane     gemini  yes  yes
Add summary method to SomaticFitness       gemini  yes  yes
Add max_depth to CausalGraph               gemini  yes  yes
Add Display impl for RiskTier              gemini  yes  yes
```

**Benchmark residue problem** — `append_benchmark_test_content` leaves dangling tests outside `mod tests {}` when the apply succeeds but the implementation is reverted by cleanup:
```rust
// BAD: test outside mod tests, left by benchmark
}  // end of mod tests
#[test]
fn test_deny_tool() { ... }  // orphaned, won't compile if impl is reverted

// GOOD: manually move inside mod tests after incorporating
    #[test]
    fn test_deny_tool() { ... }
}  // end of mod tests
```

## Related

- `docs/HANDOFF.md` — tracks benchmark state and known bugs
- `DESIGN.md` Section 2.5 — Fontana's AlChemy Level 0 threat (metric gaming)
- Karpathy's autoresearch — solved this by freezing `evaluate_bpb` (the exogenous pattern)
