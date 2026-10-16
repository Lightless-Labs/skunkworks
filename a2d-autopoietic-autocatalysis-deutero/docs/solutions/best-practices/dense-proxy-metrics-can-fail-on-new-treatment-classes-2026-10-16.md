---
title: "A Dense Proxy Metric Can Fail on a New Class of Treatment"
date: 2026-10-16
category: best-practices
module: nano/eval.py
problem_type: measurement_validity
component: experiment-design
symptoms:
  - "A candidate improves the outcome you care about by about 2.5 SE but the search metric says no change"
  - "The proxy was validated by correlation across arms that all shared one mechanism"
root_cause: proxy_validated_on_a_narrow_treatment_class
---

# A Dense Proxy Metric Can Fail on a New Class of Treatment

**Evidence:** LOG, the E002 screen and the metric amendment (cycle 2).

## What happened

- To make the keep/discard loop cheaper, the search metric was set to the **fraction of hidden tests passed** rather than all-pass.
- The case for it:
  - its A/A noise per task was a quarter of all-pass's (SD 0.12 vs 0.47);
  - it correlated 0.75 with all-pass across 51 earlier arms;
  - so it was about 2.8× more sample-efficient.
- The first candidate of a new kind, "sample 4 and let a stronger model pick", then scored:
  - **all-pass +11.8 pp** (net +8 tasks of 68, about 2.5 SE);
  - **fraction +0.005**, a discard.
- The two disagreed because of what selection does to the distribution.
  - It turns many near-misses into full passes, which gains a little fraction each.
  - It occasionally picks a file that fails catastrophically: a timeout, or a compile failure against the hidden tests, at near-zero fraction.
  - A mean of fractions is dominated by those few catastrophic tasks. All-pass counts every failure the same.
- All 51 arms behind the 0.75 correlation were repair-loop variants. They said nothing about selection-type treatments.

## Practice
- **Validate a proxy on the kind of treatment you are about to test,** not only by its correlation over past arms. If a new mechanism changes the shape of the failures (rare catastrophic failures rather than many partial ones), re-check the proxy.
- **Keep the outcome measure as the primary metric unless the proxy has been shown to track it for this treatment class.** Spend the sample-efficiency gain on screening, not on the keep decision.
- **When the proxy and the outcome disagree, the pre-registered rule still stands.** Record its decision, diagnose from data you already have (here: branch counts, fallbacks, and the fraction delta split by pass/fail quadrant), and pre-register the amendment before any further data. Then re-register the candidate and let a fresh confirmation replicate decide.
- **Calibrate every metric's own threshold from the same A/A run.** All-pass differences are integers over N tasks, so the threshold is coarse (here +7.1 pp, 10 net tasks of 140). Keep the `max(threshold, 1.645 · SE)` guard.
