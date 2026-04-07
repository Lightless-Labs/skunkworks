---
title: WorktreeCatalyst base_ref pattern — pin evaluation worktrees to a stable git ref
date: 2026-04-05
module: a2-worktree-catalyst
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - A subsystem spawns git worktrees for sandboxed execution
  - The "main" branch advances during the evaluation lifecycle
  - You want reproducible evaluation across sessions where HEAD has moved
tags:
  - worktree
  - rust
  - api-design
  - reproducibility
  - benchmark
---

# WorktreeCatalyst base_ref pattern — pin evaluation worktrees to a stable git ref

## Context

`WorktreeCatalyst` originally hardcoded HEAD as the worktree branch point. This is fine for "run candidate edit against current code," but it makes any evaluation that needs a stable substrate (benchmarks, regression suites, reproducible scoring) drift as HEAD advances.

## Guidance

Add a constructor that takes any git ref (tag, branch, or commit SHA):

```rust
impl WorktreeCatalyst {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self::with_base_ref(workspace_root, "HEAD")
    }

    pub fn with_base_ref(workspace_root: PathBuf, base_ref: impl Into<String>) -> Self {
        Self { workspace_root, base_ref: base_ref.into() }
    }
}
```

API design notes:

- `impl Into<String>` lets callers pass `&str`, `String`, or `Cow<'_, str>` without boilerplate.
- Keep `new()` as the HEAD-defaulting convenience to avoid breaking existing call sites.
- The base ref is **resolved at worktree creation**, not at construction. Resolving lazily means the caller can re-tag `bench-baseline` between catalyst constructions and the next worktree picks up the new ref.

Pair this with a tag convention like `bench-baseline` that you re-tag whenever you intentionally widen the gap between baseline and HEAD. The tag becomes the project's "frozen evaluation substrate."

## When to Apply

- Any catalyst / sandbox / worktree abstraction in a self-modifying system
- Reproducible benchmark harnesses that should not drift with HEAD
- Cases where a clean baseline is needed but the workspace is dirty by design

## Caveats

Pinning worktrees to a baseline does **not** solve the apply-back-to-HEAD problem (see `git-apply-context-mismatch-when-baseline-diverges-20260405.md`). Use this pattern for observational evaluation, not for round-tripping patches into HEAD.

## Related

- `crates/a2-worktree-catalyst/src/lib.rs`
- `docs/HANDOFF.md`
