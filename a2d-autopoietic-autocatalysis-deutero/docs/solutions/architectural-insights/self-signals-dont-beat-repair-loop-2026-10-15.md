---
title: "Self-Generated Signals Don't Beat a Repair Loop (low-effort coder, k = 5)"
date: 2026-10-15
category: architectural-insights
module: a2d-lift
problem_type: research_finding
component: harness-design
symptoms:
  - "Harness mechanisms built on the coder's own judgement fail to beat a plain public-test repair loop"
  - "Hidden failures are mostly public-green: the remaining errors are spec clauses that public tests don't exercise"
root_cause: self_signals_too_noisy_for_the_regime
---

# Self-Generated Signals Don't Beat a Repair Loop

**Regime:** `codex/gpt-6-luna` at low reasoning effort, no tools, single-file std-only Rust tasks with 15–35 numbered spec clauses, deliberately weak public tests, at most 5 model calls per task. Evidence: `docs/experiments/LOG.md`, P2 and P3 (S0–S3).

## The setup that matters

- **A plain repair loop (D)** — write, run public tests, feed failures back, stop at green — passes 67.6% (Confirmation-1, pooled). It uses about 1.35 calls per task.
- **92% of D's remaining failures are public-green.** The code passes every test it can see and fails a clause no public test exercises. Fixing them needs a signal the public tests do not give.
- **Diversity exists.** Among 5 independent samples, at least one passes hidden on 86% of tasks (D picks a passing candidate on 74%).

## What was tried, and what killed it (all offline on frozen candidate pools, about 200 calls)

| Mechanism | Result | Why it fails |
|---|---|---|
| Blind spec tests written by the coder model from the requirements | Validity 0.83, AUC 0.63 | About 17% of its tests assert the wrong behaviour. One wrong test fails every candidate (false-alarm rate 0.87). |
| The same, consensus-filtered (drop tests most candidates fail) | AUC 0.64, recall 0.28 | The filter removes the wrong tests, but takes the discriminating ones with them. |
| Clause-by-clause self-review and fix after green | conv 0.19, regr 0.03; +2.2 pp | The model rarely finds the clause it got wrong the first time. |
| CodeT-style agreement selection over 4 samples | 0.73 vs D 0.74 | The majority shares the same misreading. |

The seed A²D metabolism (P2) was no better: +1.4 pp over D, 95% CI [−5.4, +8.8], at 3.7× the tokens. Its coder never saw its own previous code, and two of its four feedback calls could not reach the submission.

## Lessons

- **Measure the selection headroom before building a mechanism.** "any-pass within the budget" minus "the repair loop's pick" bounds what selection can do. Here it was +12 pp, and no available signal could pick within it.
- **Break-even arithmetic kills ideas cheaply.** A mechanism must convert ≥ 43% of the repair loop's public-green failures, with no regressions, to gain 10 pp. Any detector with lower recall cannot reach that, whatever the repair step does.
- **Scope (gate note, 2026-10-15):** these are specific configurations that failed pre-registered development criteria, not confidence-bound exclusions of every within-task self-signal. S1 matched D's selection, and S2 had a small positive net effect.
- **Self-signals from the same model share its blind spots.** Remaining errors are mostly systematic misreadings. Other samples, its own tests and its own review all inherit them.
- **Check that every call reaches the submission.** "The component executes" is not the same as "its output can change the result". Our seed spent 2 of 4 feedback calls on outputs nothing consumed.
- **Untested, and outside the current metric:** a higher reasoning effort or a larger k; a stronger model as reviewer or test-writer (a portfolio question); stronger public tests (a different regime). Those are where lift might be, not in harness plumbing around the same low-effort coder.
