---
title: "Test Launch Gates in the Deployment Tree, and Key Them to Every Event They Guard"
date: 2026-10-21
category: best-practices
module: tools/gym/gymlib.py
problem_type: evaluation_integrity
component: launch_gate
tags: [gate, freeze, pycache, worktree, relaunch, confirmation-3]
symptoms:
  - "gate (b) passed all 59 tests in the branch worktree, but would have failed closed for good on main"
  - "gate (a) would have opened the gym window while a replicate-1 relaunch could still be generating"
---

# Test Launch Gates in the Deployment Tree, and Key Them to Every Event They Guard

## What happened
The gym v3 tooling has two launch gates: (a) a time window around confirmation-3's spend, and (b) "the frozen paths equal the registration commit, with nothing untracked there". Two reviews and a fix pass, all run in a branch worktree, missed two defects. Both came up in the merge-time check.

1. **Gate (b) would have failed closed in the real tree.**
   - Main's `tools/holdout/__pycache__/` held three `.pyc` files, written by ordinary `python3 -I` imports earlier that day. `-I` does not stop bytecode writes.
   - `git ls-files --others --exclude-standard` lists those files, so the gate would never have opened.
   - The worktree tests could not see this: a fresh checkout has no caches.
   - The spend itself may import those modules again.
2. **Gate (a) was keyed to the last scheduled event, not to every event it guards.**
   - The window opened once replicate 2 had generated.
   - Replicate 1 runs first, but its one registered relaunch can come on a later deferral day, after replicate 2. The gym could then have loaded the machine during a timed spend.

## Fix
- Gate (b) ignores only `<dir>/__pycache__/*.pyc`. Python never loads such a cache without its source, and the diff check covers the source. A sourceless `.pyc` beside the sources, or any other untracked file, still closes the gate.
- A `.gitignore` inside the frozen directory was not an option: that would be an edit under the freeze.
- Gate (a) needs every replicate to hold a non-aborted `generation_complete`, from its first attempt or its relaunch, and no scoring manifest yet.

## Practice
- **Run a gate's real-repo check in the tree it will guard,** after the merge and before relying on it. A worktree is a clean room, and the deployment tree carries runtime residue: caches, logs, lock files.
- **An untracked-files check must allow for the runtime's own byproducts,** and no more. Name the exact pattern and test both sides of it.
- **For a gate that waits for "the spend is over", enumerate every run that can still happen,** including relaunches and deferrals in any order. Do not key it to the run that is scheduled last.
