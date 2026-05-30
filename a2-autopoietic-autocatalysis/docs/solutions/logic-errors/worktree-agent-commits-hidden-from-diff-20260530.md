---
module: workcell
problem_type: logic_error
component: WorktreeCatalyst
severity: medium
date: 2026-05-30
tags: [worktree, diff, lineage, provider, pi, benchmark]
applies_when:
  - "A worktree agent reports/verifies a fix but A² records an empty patch"
  - "A model may commit changes inside the temporary candidate worktree"
---

# Worktree agent commits can hide patches from staged-only diff capture

## Symptom

A `compound-raf-same-crate-hidden` Pi/ZAI self-correction run resolved the verifier on attempt 1, but the lineage record had no touched files, no diff stats, and `lineage_reconciled_by_core=false`. The workcell reported that the agent made no changes even though the final fixture verifier passed.

## Cause

`WorktreeCatalyst::capture_diff()` staged uncommitted worktree changes and then ran:

```bash
git diff --staged --no-color
```

That captures unstaged/staged edits, but it does not capture edits that the agent commits inside the temporary worktree branch. In that case the working tree is clean relative to `HEAD`, so the staged diff is empty even though the candidate branch differs from the original base commit.

## Fix

Resolve the temporary worktree's base commit before running the agent:

```bash
git rev-parse HEAD
```

After the agent returns, stage uncommitted changes and diff the worktree against that pre-agent base commit:

```bash
git add -A
git diff --no-color <base-commit>
```

This captures uncommitted, staged, and committed worktree changes in one patch.

## Regression test

Add a WorktreeCatalyst test that edits a temporary worktree, commits the edit, and asserts that `capture_diff()` still contains the committed line.

## Reporting discipline

If a benchmark record has `resolved=true` but an empty patch/diff and no reconciliation, treat it as a capture-path signal, not clean evidence that A² promoted a useful patch. Record the exact JSONL fields and inspect the worktree capture path before drawing benchmark conclusions.
