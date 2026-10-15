# Plan: Cycle 2 — Autoresearch on a Minimal Coding Harness

**Created:** 2026-10-15
**Status:** Active
**Goal:** the session `/goal` of 2026-10-15 ("A²D cycle 2"). This plan is its working form.
**Reviewed:** 2026-10-15, design review before the first live run (Codex `gpt-6.1-sol` high, fresh Claude subagent; copies-only dirs). Dispositions in the Addendum.
**Addendum:** 2026-10-15 — **design review dispositions and definitions** (all accepted unless noted):
- **Evaluator integrity** (`nano/eval.py` v2.1):
  - experiments are immutable, with a manifest of code, controller and suite hashes, and the frozen snapshot is what gets promoted;
  - every scored run needs exactly one score per expected task and arm, or the run is invalid;
  - the A/A hash must match the current incumbent, and the A/A is re-run after every keep;
  - the screen half is rotated per experiment id;
  - structured fields: stage, n tasks, n families, calls by lane, tokens.
- **Keep rule:**
  - provisional if Δfrac > max(A/A threshold, 1.645 · cluster SE), and the all-pass Δ is not significantly negative;
  - **promotion only after a fresh confirmation replicate with Δfrac > threshold / 2**.
  - Keeps are exploratory; the reviewers' estimates are about 5% false keeps per test before confirmation and about 1% after.
- **D parity:**
  - a zero-call fixture test (native D vs external v0 under a deterministic mock: identical prompts and identical submission) is required before any scored run;
  - the A/A is a 3-arm live run (D, X:a, X:b) whose D-vs-X deltas must lie within ±5 pp all-pass, with byte-identical first prompts.
- **E002 controls:**
  - **E003**: the same 4-sample policy with a random-green selector. E002 − E003 attributes the gain to the judge.
  - Claim wording: **"Luna generation plus Sol selection"**. Sol is a stronger model from the same vendor, not a different family.
  - Reported per arm: calls and tokens by lane, branch frequencies and fallbacks.
  - A cost-matched control (D plus one Sol call) comes before any resource-efficiency claim.
- **Gate metric** (to the confirmation set):
  - the frozen incumbent's **all-pass** lift vs D on the dev-holdout is ≥ +10 pp as a point estimate, at r = 2, with a paired family-cluster CI lower bound > 0.
  - Dev-holdout looks: one after each confirmed keep (at most 4), plus a final frozen-candidate look. Proposers learn only pass or fail.
  - This replaces "every 10 keeps", which a 25k budget would almost never reach.
- **Proposer comparison** (A²D's architect vs uninformed): primary metric = the mean paired Δ per proposal against the same frozen incumbent, in interleaved blocked rounds; keep rate is secondary. The protocol is frozen and pre-registered before it runs, including an inconclusive outcome.
- **Authorship confound:** the train and dev-holdout tasks are Sol-authored, and Sol is also the live judge. This is disclosed. A Claude-lane judge replication is needed for a different-family claim (the CLI is unauthenticated from subprocesses).
- **Not adopted:** O'Brien–Fleming group-sequential bounds. The confirmation replicate plus exploratory labelling covers the same risk more cheaply.

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
