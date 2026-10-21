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
- **First attempt, withdrawn the same evening.** Gate (b) ignored `<dir>/__pycache__/*.pyc`, on the reasoning that a cache is never loaded without its source and the diff covers the source.
  - A narrow Codex Sol review rejected it as a blocker. Python loads a timestamp-based `.pyc` whose header matches the *unchanged* source's mtime and size, *instead of* that source. An unchecked-hash `.pyc` is loaded without any check.
  - So an untracked cache can substitute code while the diff stays clean.
- **Final fix:**
  - Gate (b) is strict again, caches included.
  - The three existing caches were moved aside.
  - Their producer stopped writing caches. The queue's confirmation-3 lines run `python3 -I -B` and export `PYTHONDONTWRITEBYTECODE=1`, because `-I` ignores the variable but child pythons may not run with `-I`. Manual `confirm3.py` runs use `-B` too.
  - A `.gitignore` inside the frozen directory was not an option: that would be an edit under the freeze.
- **Gate (a)** needs every replicate to hold a non-aborted `generation_complete`, from its first attempt or its relaunch, and no scoring manifest yet.

## Practice
- **Run a gate's real-repo check in the tree it will guard,** after the merge and before relying on it. A worktree is a clean room, and the deployment tree carries runtime residue: caches, logs, lock files.
- **Do not loosen an integrity check to tolerate a byproduct; stop producing the byproduct.**
  - A cache, a compiled artefact or a plugin directory is often *executable*, so tolerating it reopens the hole the check closes.
  - Find the producer (here, `python3` imports of the frozen modules) and switch it off (`-B`).
  - Keep the check strict, and fail closed.
- **Send even a "trivial" post-review loosening of a sealed gate to review.** This one looked obviously safe and was not.
- **For a gate that waits for "the spend is over", enumerate every run that can still happen,** including relaunches and deferrals in any order. Do not key it to the run that is scheduled last.
