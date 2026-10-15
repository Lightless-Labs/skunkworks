---
title: "Controls, Noise and Boundaries for a Self-Improving Harness Search"
date: 2026-10-15
category: best-practices
module: a2d-lift
problem_type: process_lesson
component: experiment-design
symptoms:
  - "A self-improvement search accepts candidates and the result reads like learning"
  - "Candidates beat the system's own base but not the external baseline"
  - "A result sits exactly on a classification boundary"
root_cause: acceptance_noise_and_missing_controls
---

# Controls, Noise and Boundaries for a Self-Improving Harness Search

Sources: `docs/experiments/LOG.md` E10b, E11, E12 and the roadmap close; `docs/reports/2026-10-15-lift-first-outcome.md`.

## 1. An equal-budget control and an A/A arm stop "gains against our own base" being read as learning

- **What happened.** In P4 (E11, E12), A²D's architect wrote lessons from its own train failures, and a ratchet accepted a candidate when it beat the incumbent by at least 4 tasks on a held-out half.
  - Both informed searches accepted a candidate.
  - On validation, the best one beat its own base by +11 pp (CI [+1, +22]).
  - Read alone, that looks like learning.
- **What the control showed.** An uninformed control at the same budget wrote lessons from requirements only, with no failure labels, and went through the same ratchet.
  - It accepted as often (2 of 6 comparisons vs 2 of 6).
  - It tied on validation (55/80 each), and beat the base by the same +11 pp.
  - Neither beat the external baseline D.
- **Practice:**
  - Run an equal-budget control through the same acceptance rule.
  - Add an A/A arm that evaluates the incumbent as its own candidate. That measures the false-accept rate directly.
  - Judge against the external baseline, not only against your own base. A weak base makes almost any addition look good.

## 2. Measure the decision noise before searching, and size decisions to the effect

- **The noise.** D's test-retest flip rate on 140 train tasks was 0.229: two runs of the same harness disagree on 23% of tasks. Two runs of an identical incumbent on the same 68-task half scored 45 and 40.
- **What it does to the ratchet.** At that noise, "accept at ≥ 4 net of 70" has P(accept | no effect) = 0.19 per comparison. That gives P(≥ 4 of 12 accepted) = 0.18, which is what we observed.
- **What it costs to beat it.** Resolving ±5 pp at power 0.8 needs about 700 paired tuples per decision, before family clustering. Replicates reduce sampling noise, not task-sampling variance.
- **Practice:** before a search, simulate the implemented acceptance rule with the measured flip rate and family structure. Pre-register the target effect, error rates and sample size. If the budget can't reach that size, search for larger effects or change the regime.

## 3. Classify count-ratio boundaries with a tolerance

- **The bug.** A lift of exactly 8/80, computed as `x/n − y/n`, is `0.09999999999999998`. The classifier's `ci_high < 0.10` therefore classed an exact 10.0 pp upper bound as RuledOut instead of Inconclusive (E12 exploratory comparisons).
- **The fix.** `stats::classify` compares every boundary with a 1e-9 tolerance, and a unit test reproduces the 8/80 case.
- **Practice:** any threshold on rates of counts needs either a tolerance or integer arithmetic. Test the exact-boundary case.
