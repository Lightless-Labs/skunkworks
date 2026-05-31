---
title: "Autopilot Markdown Claims Need Repo-Path Validation"
date: 2026-05-31
module: autopilot
tags:
  - autopilot
  - validation
  - markdown
  - project-references
problem_type: runtime-bug
---

# Autopilot Markdown Claims Need Repo-Path Validation

## Problem

The autopilot repair path could produce a typed, non-empty markdown patchset that passed path gates, temp validation, real-tree `cargo test`, and commit, while still making false planning claims about repository files. The DeepSeek repair validation run `run-1780125199376-0` updated a todo with source-file references that did not exist (`metabolism_workcell.rs`, `provider_registry.rs`).

Mechanical gates proved the patch was parseable, local, testable, and committable. They did not prove that documentation claims about the project were true.

## Fix

Autopilot temp-worktree validation now performs a semantic repo-reference pass for markdown outputs:

- scans markdown replacements and `handoff_update` text;
- extracts repo-path references such as `crates/...`, `docs/...`, `todos/...`, `examples/...`, `research/...`, and root manifest/doc files;
- normalizes markdown anchors and line suffixes (`docs/plan.md#section`, `src/file.rs:42`);
- rejects unsafe references and references that do not exist after the patch is applied to the temp worktree;
- routes failures through the existing validation/repair budget before any real-tree apply or commit.

Maintainer and repair prompts now also state that invented repo paths will fail validation.

## Coverage

Added unit coverage for both sides of the gate:

- an accepted markdown patch that references existing files with an anchor and line suffix;
- a rejected markdown patch matching the observed failure shape: non-existent `crates/a2d-core/src/metabolism_workcell.rs` and `crates/a2d-core/src/provider_registry.rs` references.

Validation:

```text
cargo test
36 CLI tests + 134 core tests + 11 bootstrap + 7 provider + 1 doctest = 189 passing, 2 ignored
```

A dirty-tree dry-run smoke also confirmed autopilot still builds project state, selects the next task, emits monitor artifacts, and stops before provider invocation with `--dry-run --allow-dirty`.

## Lesson

For autonomous documentation/planning work, path safety is not semantic safety. If a patch can influence future task selection or implementation direction, repository-reference claims should be mechanically checked against the post-patch tree, not trusted because markdown syntax and tests pass.
