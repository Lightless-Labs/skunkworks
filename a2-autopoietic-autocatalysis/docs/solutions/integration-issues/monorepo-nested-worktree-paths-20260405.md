---
title: Worktree paths must be relative to git root, not the nested crate
date: 2026-04-05
module: a2ctl-baseline
problem_type: integration_issue
component: tooling
severity: medium
symptoms:
  - "baseline.sh fails with 'no such file or directory' on cd into worktree"
  - "Paths inside the worktree exist but at a different prefix than expected"
  - "Scripts that work standalone fail when the project is moved into a monorepo"
root_cause: config_error
resolution_type: code_fix
related_components:
  - worktree-catalyst
  - benchmark
tags:
  - monorepo
  - worktree
  - bash
  - paths
  - portability
---

# Worktree paths must be relative to git root, not the nested crate

## Context

A² lives at `skunkworks/a2-autopoietic-autocatalysis/` inside the Lightless Labs monorepo. `git worktree add` creates a worktree of the **entire repository** rooted at the git toplevel — not at the subdirectory you happen to be running from. Scripts that did `cd "$WORKTREE/src"` worked standalone but broke once nested, because the actual path was `$WORKTREE/skunkworks/a2-autopoietic-autocatalysis/src`.

## Resolution

Compute the relative path from the git toplevel to the current crate dynamically and prefix every worktree path with it:

```bash
GIT_ROOT="$(git rev-parse --show-toplevel)"
HERE="$(pwd)"
REL="$(python3 -c "import os,sys; print(os.path.relpath(sys.argv[1], sys.argv[2]))" "$HERE" "$GIT_ROOT")"

# REL is "" when not nested, "skunkworks/a2-autopoietic-autocatalysis" when nested.
WORKTREE_CRATE="$WORKTREE/$REL"
( cd "$WORKTREE_CRATE" && cargo test )
```

`python3 os.path.relpath` is preferable to `realpath --relative-to` because the latter is GNU-only and missing on stock macOS. `python3` is universally available on dev machines and gives the correct empty-string result when the paths are equal.

## Why This Matters

This bug is invisible until the project is moved or vendored. Any script that hardcodes a relative path inside a worktree is a portability landmine. The general rule: **`git worktree` mirrors the git root, never the cwd**, and any script that walks into a worktree must compute its target path from the git toplevel.

## Examples

Before (broken in monorepo):
```bash
cd "$WORKTREE" && cargo test   # runs at the wrong workspace, no Cargo.toml here
```

After (works both standalone and nested):
```bash
cd "$WORKTREE/$REL" && cargo test
```

## Related

- `scripts/baseline.sh`
- `crates/a2-worktree-catalyst/src/lib.rs`
