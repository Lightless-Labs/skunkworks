# Noise floor of the seed genome G0, and the pre-stated keep/discard rule

**Date:** 2026-10-16. Committed before the first candidate genome is evaluated.
**Run:** `nf-1`, label `G0`: the seed genome `genome/` at the commit before the loop starts. Rows are private, under `~/.a2-e1/private/evolve-runs/nf-1/`.
**Tasks:** the 44 dev tasks (E1's ansible pool), graded by the hardened grader. The pool gates 44/44 (`hardened-gate-dev.json`).
**Rule:** plan §5, using `bench/evolve/stats.py`:
`python3 bench/evolve/stats.py noise-floor --run-dir … --label G0 --seed 1016 --hard 12 --sentinels 3 --trials 1000`

## What G0 does

**Runs and resolution:**
- 106 episodes in total: 2 runs on every task, plus 3 to 4 more on the 6 tasks that were split or had an infrastructure failure.
- 102 episodes were clean. The other 4 were `provider_error` after the one retry, and are excluded.
- Resolve rate: run 0 resolved 18/48 and run 1 resolved 19/49, about 38%.

**Per-task resolution:**

| Successes / runs | Tasks |
|---|---|
| 0/2 | 25 |
| 0/4 | 1 |
| 2/2 | 14 |
| 1/5, 2/5, 3/5, 4/5 | 1 each |

G0 is close to deterministic per task. Only 4 tasks are contested, so any measurable improvement has to come mostly from cracking tasks that G0 never solves.

**Cost per episode:**
- 0.94 M tokens on average; 95.5 M tokens for the whole run.
- Wall time: median 989 s, p90 2306 s. This was measured while the sealed gate shared the disk.
- Agent calls per episode: 1 call in 19 episodes, 2 in 34, 3 in 49.

## Screen set S, fixed for the campaign

S has 19 tasks, listed by opaque id in `screen-set.json`, with seed 1016:
- the 4 contested tasks;
- 3 always-solved sentinels, so that regressions count;
- 12 never-solved hard tasks.

The champion's G0 rates are in `g0-rates.json`.

**Why 12 hard tasks, not the plan's 4.** This addendum is dated 2026-10-16 and was made before any candidate was evaluated. Simulation on these rates, with 400 trials per cell, gave:

| Hard tasks | \|S\| | False keep | P(keep), logit +0.5 | logit +1.0 | logit +1.5 |
|---|---|---|---|---|---|
| 4 | 11 | 0.015 | 0.085 | 0.26 | 0.48 |
| 8 | 15 | 0.020 | 0.098 | 0.385 | 0.63 |
| **12** | **19** | 0.020 | 0.205 | 0.445 | 0.735 |
| 25 | 32 | 0.010 | 0.193 | 0.595 | 0.905 |

Twelve hard tasks nearly doubles power over the plan's 4 at the same false-keep rate. A screen then costs about 18 M tokens. Using all 25 would make each screen 70% more expensive, and tokens are the binding constraint. The plan's expected |S| was 18–20.

## Thresholds, fixed before any candidate

These come from the Monte Carlo null: candidates identical to G0, with p̂ resampled from Jeffreys posteriors on the G0 runs.

- **Stage A screen**, one candidate run on S: proceed iff Δ_A ≥ **2.0**, the null's 70th percentile.
- **Stage B**, two fresh runs on S: keep iff the null p ≤ 0.05, which here means Δ_B ≥ **4.0**. The candidate must also not regress on the full pool: one run on all 44 tasks with Δ ≥ **−1.0**, the null's 10th percentile.

Simulated over the whole rule with the final rates, 1000 trials:

| Effect (logit shift per task) | P(keep) |
|---|---|
| 0 (null) | **0.017** |
| +0.5 | 0.12 |
| +1.0 | 0.40 |
| +1.5 | 0.76 |

Across about 20 experiments, a 1.7% false-keep rate allows at most about one spurious keep. The sealed confirmation (protocol-v2) is the only promotion claim. Changes smaller than about one logit per task, such as prompt polish, are below what this loop can see, and `program.md` says so.

## Evaluator changes during the run

The noise-floor rows carry three `evaluator_sha256` values, because the evaluator was fixed between batches. None of the fixes changes how an ansible dev task is graded or what the policy sees on a successful episode:
- task-id scrubbing in verifier text: this only removed a crash;
- episode errors recorded as infrastructure rows;
- plugin autoload, which gives the same plugin set (`pytest_mock`) for ansible venvs;
- import roots restored after pytest is pinned: ansible's roots `lib` and `test/lib` were never dropped;
- qutebrowser and openlibrary profile fixes, which do not apply to ansible.

From here on, every loop row is checked against `bench/evolve/evaluator.lock`.

## Addendum, 2026-10-16: what is fixed and what floats (from the evaluator audit)

What is pre-registered is the **rule**, not the numbers 2.0 and 4.0. The rule is plan §5 as implemented in `stats.py`:
- stage A passes at the 70th percentile of the one-run null;
- stage B keeps at null p ≤ 0.05, plus the full-pool floor at the 10th percentile;
- the null is drawn by Monte Carlo from Jeffreys posteriors on the **current champion's fresh runs**.

The thresholds quoted above are what that rule gives on G0's noise-floor rates. Before the next experiment that comes after a nightly control run or a keep, the loop recomputes them from the current champion's rates, as plan §5 specifies.

Two corrections from the same audit apply from the next experiment on:
- A task the champion never ran cleanly is left out of both the null and the observed Δ. Previously such a task crashed the decision.
- A candidate's infrastructure-failed task is imputed at the champion's p̂, which makes it neutral. Previously it was scored 0, which biased decisions against the candidate.

## Addendum, 2026-10-17: the audit's evaluator changes are in force from exp-0004

The loop was stopped at the experiment boundary after exp-0003, and the remaining audit fixes were installed. These are the changes, under evaluator sha256 `6ab0fe7e…`:
- **Live result stream.** The guard plugin writes each test phase result, followed by an END marker, to an inherited pipe. A junit pass now counts only if the stream confirms it. Any tampering, or a record written after END, voids the run's passes. This closes the hole where a candidate could forge junit at interpreter exit.
- **Stats corrections.** The two corrections in the previous addendum now apply.
- **`cfg.deny_read` honoured.** The episode now uses `cfg.deny_read`, which the confirmation runner needs.
- **Cleanup on errors.** A crashed episode's work dir is removed.

Before the swap, with no loop running, all 44 dev tasks were re-gated under the new grader. 44/44 pass: gold resolves and the empty diff does not. G0's noise-floor rates were graded by the old grader, and they stand. The new check only removes passes that the stream does not confirm, and honest runs produce a complete stream: gold does on all 44 tasks. Nightly control runs re-measure the champion under the new grader.

## Addendum, 2026-10-17: rename and binary audit, in force from exp-0006

The loop was stopped at the boundary after exp-0005, while exp-0006 was in the researcher stage. The researcher stage restarts from scratch on resume. The evaluator sha256 is now `63404eaa…`. Changes:
- Candidate diffs are generated with `--no-renames`, so moved content appears as added lines and is scanned.
- `audit_diff` rejects rename and copy headers, and binary patches outside test paths.
- `bench/confirm` is now part of the lock.

None of the 193 gold patches (68 dev, 125 sealed) contains a rename, copy or binary section, so every gate result is unchanged.

## Addendum, 2026-10-20: infrastructure classifier fix, in force from exp-0010

exp-0010 tripped a classification bug. Its first call ran the reproduce-only prompt, and the agent often made no edits. a2ctl reports `tokens=0` for a call like that, and the evaluator read any zero-token attempt as a `provider_error`. A provider error marks the episode as infra, so it was retried, and dropped from scoring if the retry also failed. In exp-0010's stage A, 17 of 19 rows were flagged this way.

The same bug touched every earlier run. Across all evolve logs, the zero-token calls were 74 no-edit calls, 68 timeouts and 3 genuine provider exits. Under the pre-registered definition, timeouts and no-edit calls are agent failures. The classifier now reads the call logs (see protocol-v2 `[addendum_2026_10_20]`); evaluator sha256 is `99fac147…`.

**Champion rates.** The loop was stopped while it was backing off. `champion_rows` in the loop state were reclassified from their logs. Only rows previously flagged `provider_error` were reclassified; `evaluator_error` rows stay infra. 7 rows changed (4 in nf-1, 3 in the 2026-10-18 control run). Per-task rates (resolved / runs) before and after:

| task | before | after |
|---|---|---|
| e1-237cd392e5f3 | 2/2 | 5/5 |
| e1-32a5795ee41f | 0/4 | 0/5 |
| e1-2f91f4386c9f | 0/5 | 0/6 |
| e1-667916f5ba15 | 5/5 | 5/6 |
| e1-82b8da935f94 | 1/8 | 2/9 |

Some originals cannot be recovered. The evaluate retry overwrote the original episode row, so a retried episode's first grade is lost. 10 champion rows are retries that this bug triggered (8 in nf-1, 2 in the 2026-10-19 control run). One of them, on e1-792e600f54fd, followed a genuine provider exit. All 10 are on tasks that are deterministic in the remaining reps (every rep resolves, or none does). The lost originals therefore cannot move p̂, and the floor is not re-run. The other 13 retry rows in the state come from the `evaluator_error` retries of 2026-10-18, a separate and genuine infra class, and are unaffected.

**Earlier experiments.** exp-0001 to exp-0009 were decided under the old classifier. On both sides, agent timeouts and no-edit calls got free retries. Those decisions stand as logged. exp-0010's stage A was deleted and re-runs under the fixed classifier. It is still exp-0010, not a new experiment. Its logs were deleted with the run before being archived; the counts above were taken first.

## Addendum, 2026-10-21: re-run of exp-0012 after an infrastructure failure (pre-registered)

exp-0012 passed stage A. It was then logged `invalid` because the disk filled during stage B and `tasks.prepare` failed (`git commit` exit 128). That is an infrastructure failure, not a result about the genome ([blocker](../../blockers/2026-10-20-disk-full-exp-0012.md)). This addendum fixes the re-run before any of it is executed.

- **What runs:** the archived diff `archive/exp-0012.patch`, applied to G0 (its logged parent), as experiment `exp-0012r`. The hypothesis text is unchanged, and no researcher call is made.
- **From scratch:** stage A, stage B and the full-44 pass all run fresh, with new run ids `exp-0012r-A/-B/-full`. The original exp-0012 rows (19 in A, 34 partial in B) are not reused. Some of them were produced on a full disk.
- **Same rules:** the same evaluator (`99fac147…`), the same screen set, the same thresholds, and the same `stats.screen_decision` / `stats.stage_b_decision`. The comparison is against the champion rates in the loop state when the re-run starts. If the champion has changed by then, the comparison is against the new champion.
- **When:** after the loop reaches its 22nd logged experiment and exits, so that the two never run concurrently on this machine. It is injected as the loop's `current` experiment at `stage_a`, and the unchanged `loop.py` runs it with `--max-experiments` raised by one.
- **Counting:** the original exp-0012 row stays `invalid` in the log. exp-0012r gets its own log row and counts as a re-run, not as one of the 20 unattended experiments. A `keep` promotes it through the normal path and makes it the confirmation candidate under the plan §7 checklist.
- **One shot:** exp-0012r is run once. If it is also invalidated by infrastructure, that is reported and it is not run a third time.
