# Plan: Cycle 2 — Autoresearch on a Minimal Coding Harness

**Created:** 2026-10-15
**Status:** Active
**Goal:** the session `/goal` of 2026-10-15 ("A²D cycle 2"). This plan is its working form.
**Lineage:** the lift-first roadmap (`docs/plans/2026-10-14-lift-first-roadmap.md`, Completed (falsified)), report `docs/reports/2026-10-15-lift-first-outcome.md`. That closed one plan, not the project.

## Outcome sought

Harness changes that make our coder (`codex/gpt-6-luna`, low effort) produce working software more reliably than its plain repair loop D, at the same per-task invocation budget. Then: does A²D's own proposer find such changes better than an uninformed proposer?

## Why this shape
- In the current regime the public tests' information is spent. D reaches public-green on about 99% of tasks, and its remaining failures are spec misreadings no public test exposes. A harness can only add information or compute (cycle-1 report § Answer).
- Cycle 1 iterated slowly (about 200 calls and 45 minutes per decision) against noise larger than its effects (D's retest flip rate is 0.229).
- **Hidden-test pass fraction** is about 2.8× more sample-efficient than all-pass:
  - per-task A/A spread 0.12 vs 0.47;
  - it tracks all-pass at 0.42 per point, with correlation 0.75 over 51 arms;
  - about 250 paired tuples resolve +5 pp at 0.8 power, against about 700 for all-pass.

## Pieces

1. **Instrument.** The controller, oracle, ledger and holdouts are unchanged in their semantics. One extension: **external arms** `X:<name>`.
   - The controller runs a Python harness file per task over JSON lines.
   - Its model calls are metered, its public tests go through D's oracle, and it never sees hidden tests.
2. **Nanoharness** (`nano/harness.py`): v0 = D, byte for byte. Each experiment is one edit to this file. Everything A²D had before (metabolism, enzymes, lessons, evolver) comes back only as a kept experiment.
3. **Eval v2** (`nano/eval.py`, with results in `nano/results.tsv`):
   - Dev set: `tasks/train-all` (140 tasks, 35 families). Dev-holdout: 25 new blind families (`$A2D_HOLDOUT_DIR/dev-holdout`), metric-only.
   - Incumbent and candidate run in ONE run (`--arms X:inc,X:cand`), paired by task.
   - Metric: mean paired difference in hidden-test fraction. All-pass is also reported.
   - **Screen** on half the dev families. A candidate that clears half the threshold is run on the full dev set.
   - **Keep** if the full-set delta is above the 95th percentile of the A/A distribution (incumbent vs itself, same protocol).
   - One line per experiment in `results.tsv`: id, hypothesis, delta, SE, keep/discard, invocations.
4. **Information source** (experiment 1, C2-E1): a cross-family judge over Luna's candidates decides whether cross-model review is the first lever, or whether the loop moves to a tool-using multi-file regime.

## Loop
- One change per experiment; the hypothesis is logged before the run; discards are logged too.
- After every 10 keeps, re-check the incumbent vs D on the dev-holdout.
- **Proposers:** me, Codex Sol, and the human-insight seed list. After ≥ 3 keeps, A²D's own architect proposes, against an uninformed proposer at equal budget. The self-improvement result is their keep-rate comparison.
- **Claims:** confirmation sets are spent once, only on a frozen harness with a dev-holdout lift vs D of ≥ +10 pp, with D and B as comparators, after one Codex Sol plus fresh-Claude review. Confirmation-2 is valid only in the current regime.

## Budget
At most 3k Luna invocations a day, and at most 25k in total. Deadline 2026-11-30. Spend is tracked in `results.tsv` and the LOG.
