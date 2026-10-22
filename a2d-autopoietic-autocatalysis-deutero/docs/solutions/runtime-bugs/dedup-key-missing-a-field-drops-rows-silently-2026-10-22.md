---
title: "A De-duplicating Logger Whose Key Misses an Identifying Field Drops Rows Silently"
date: 2026-10-22
category: runtime-bugs
module: nano/eval.py
problem_type: data_loss
component: results_log
tags: [results.tsv, log_result, dedup, phase-b, backfill, bookkeeping]
symptoms:
  - "phase B ran 4 rounds x 2 arms, but nano/results.tsv held 4 rows, all pc-informed"
  - "no error, no warning, no count: the uninformed rows simply never appeared"
---

# A de-duplicating logger whose key misses an identifying field drops rows silently

## Problem

`nano/eval.py:log_result` appends one row to `results.tsv`. A resumed experiment reaches the same
decision again, so the function skips a row whose `(id, stage, decision)` is already logged.

Phase B (`c2_phaseb.py round`) logs one row per arm. Both rows use id `PC-<r>` and stage `round`,
and they differ only in `proposer` (`pc-informed` / `pc-uninformed`). Both arms screen-failed in
all four rounds, so the second row's key always matched the first and was skipped, every time.
The log is the cycle's single source of truth ("one line per experiment, discards included"),
yet it showed only half of phase B.

It surfaced at close-out, when results.tsv gained one row for round 4 instead of two.
`c2_phaseb.py` itself de-duplicates on `(id, stage, proposer)`, so its own check passed. The
shared logger underneath it used a narrower key.

## Why it stayed hidden

- The skip is silent: there is no return value and nothing is printed.
- A row only goes missing when two keys collide. If either arm had screen-passed or been invalid
  in any round, its row would have been logged, and the log would have looked plausible.
- No consumer was affected, so nothing broke:
  - the informed brief filters `pc-uninformed` rows out by design;
  - the endpoints read `scores.json`.

## Fix

- **Restore from the registered source, not by hand.** `docs/experiments/analysis/c2_phaseb_backfill.py`
  rebuilds each row from `scores.json` with the same functions the round used (`paired_table`,
  `ev.mean`, `ev.cluster_se`, `ev.sha`).
- **Prove the reconstruction before writing.** The script first rebuilds the four rows that *were*
  logged (informed) and requires a field-for-field match. Only then does it append the missing
  rows, with a provenance note.
- **Make it idempotent.** A re-run appends nothing.
- **Fix the logger after the freeze** (`todos/results-log-dedup-key.md`). `eval.py` is frozen
  until confirmation-3's decision.

## Practice

- **A dedup key must contain every field that identifies a row.** Ask: can two distinct events
  share this key? Here the key held a *result* (the decision) but not an *identity* field (the
  proposer).
- **Never drop a row silently.** Return or print "skipped (duplicate)", so that the caller can see
  it.
- **When two layers both de-duplicate, the narrower key wins.** Check the logger the caller relies
  on, not only the caller.
- **Count, don't eyeball.** At close-out, compare the expected row count (rounds × arms) with the
  actual one.
- **Check a backfill against the rows that survived.** If the code cannot reproduce the rows you
  still have, its reconstruction of the missing ones proves nothing.
