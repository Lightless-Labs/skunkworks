---
title: "Power-Plan the Implemented Decision Rule, Not a Textbook Test"
date: 2026-10-14
category: best-practices
module: a2d-lift
problem_type: experiment_design
component: statistics
symptoms:
  - "Plan says '80% power at 10 pp' but the classifier also requires point estimate >= 10 pp"
  - "Replicates judged useless from per-arm run-to-run agreement"
root_cause: power_formula_not_checked_against_classifier
---

# Power-Plan the Implemented Decision Rule, Not a Textbook Test

## What happened

The roadmap required "80% power at 10 pp". I planned the Confirmation-1 size with a paired-binary (McNemar-style) formula. It reported roughly 162 independent pairs, so I recorded "about 75% power at 148 tasks".

The classifier actually used (`stats::classify_guarded`) declares SUPPORTED only when **both** of these hold:

- the 95% lower bound is above 0;
- the **point estimate** is at least 10 pp.

At a true lift of exactly 10 pp, the point estimate clears 10 pp about half the time, whatever n is. So P(SUPPORTED | 10 pp) is about 0.5 at any sample size. The formula answered a different event: P(lower bound > 0).

A second inference went wrong too. I read "replicates add little" off per-arm agreement across runs (A 0.79, B 0.92). But what matters is how noisy the **paired difference** is. At agreement around 0.8, run-to-run noise alone reproduces the pilot's discordance, and two replicates are worth about as much as doubling the number of families.

The P1 gate's Claude reviewer caught both. It simulated the implemented rule: whole-family percentile bootstrap, 37 families × 4 tasks. The results for P(SUPPORTED), P(RULED OUT) and P(INCONCLUSIVE) were:

| True lift | Approximate outcome probabilities |
|---|---|
| 0 | RULED OUT ≈ 0.50–0.74 at 1 replicate; ≈ 0.96 at 2 replicates with agreement 0.79 |
| 10 pp | SUPPORTED ≈ 0.50 at any n |
| 15 pp | SUPPORTED ≈ 0.82–0.94 |

## Practice

- Plan power by **simulating the exact classifier the controller runs**: the same bootstrap, clustering, floors and thresholds. Use a formula only as a sanity check.
- State the targets as event probabilities on the decision rule, for example P(SUPPORTED | δ) and P(RULED OUT | 0). A bare "power at δ" is ambiguous.
- Estimate replicate value from paired-difference noise. To measure it, rerun the comparison, not one arm.
- Record any change to these in an addendum **before** registering the confirmation run. After scoring, it is too late.
