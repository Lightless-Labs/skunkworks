---
title: "Once the Selector Hits the Any-Pass Ceiling, the Bottleneck Is Generation"
date: 2026-10-16
category: architectural-insights
module: a2d-lift / nano
problem_type: research_finding
component: harness-design
symptoms:
  - "A kept sample-then-judge harness scores exactly the any-pass rate of its own samples"
  - "Remaining failures are shared misreadings: every sample is wrong the same way, so no pick can be right"
root_cause: selection_headroom_spent_so_remaining_error_is_correlated_generation
---

# Once the Selector Hits the Any-Pass Ceiling, the Bottleneck Is Generation

**Evidence:** LOG E002b (live, confirmed keep) and C2-FA1 (offline re-scoring of E002's runs, zero model calls). Follow-up to `stronger-reviewer-sees-what-self-signals-miss-2026-10-15.md`.

## The setup

- Harness E002: 4 independent Luna samples (`codex/gpt-6-luna`, low effort), public-tested, then one Sol (`gpt-6.1-sol`) call picks among the distinct public-green files from the spec alone.
- It beat Luna's plain repair loop D on the dev set: +10.7 pp all-pass (full stage), +7.1 pp on a fresh confirmation replicate.
- A random-green control (same 4 samples, random pick) scored -1.4 pp, so the judge is the active ingredient.

## The finding

- C2-FA1 re-scored every coder sample of E002's 280 task-runs on public and hidden tests.
- In 249 judge-branch task-runs the judge never picked a hidden-failing file when a hidden-passing green existed: 0 misses.
- E002's all-pass (0.857) equals the any-pass ceiling of its own samples. D scored 0.768 in the same runs.
- The 40 failures: 21 judge branch with every green failing hidden (a shared misreading), 13 one-green branch whose green fails hidden, 6 with no green.
- By kind: 30 partial hidden failures, 8 timeouts, 2 hidden compile failures.
- Any-pass@k: k=1 0.643, k=2 0.764, k=3 0.819, k=4 0.850. The 4th sample is worth about +3 pp.

## Lessons
- **Re-measure the any-pass ceiling after every kept selector.** If the selector sits on it, better judge prompts or checklists have nothing to buy. Here they were deprioritised.
- **The bottleneck moves to generation.** A misreading shared by all samples defeats any selector. The levers are decorrelating the samples, or adding information before generation.
- **Under a fixed call cap, a call moved from sampling to analysis costs the marginal any-pass of the last sample** (about 3 pp here). That is the bar any such trade must beat.
- **Measure it offline first.** The whole analysis re-scored existing runs and needed no model calls, before any live budget went to the next experiments.
- **Next experiments registered from it:** E005 (diverse sampling angles) and E006 (Sol spec-analysis notes before 3 samples).
- **Scope:** exploratory, dev set. Sol and Luna share a vendor, so this is not a cross-family claim, and the run is not cost-matched (about 6.4M vs 1.4M tokens per 140 tasks).

## Addendum 2026-10-17: reading a generation-side candidate's lift (E005)
- **E005** (samples 2–4 each get a different "Approach" paragraph) is a provisional keep: +5.0 pp all-pass vs E002 at the full stage, 123 vs 116 of 140. The same re-scoring split that gain into parts:
  - **generation +5 tasks:** more tasks had a public-green, hidden-passing sample, any-pass@4 0.857 vs 0.821;
  - **selection +2:** the in-run E002 arm lost 2 tasks to the judge, E005's lost none, with the same judge prompt, so this part is noise;
  - **no-green repair 0.**
- **Lesson 1: split every lift into generation, selection and repair** before believing the mechanism.
  - A generation-side change should move only the generation term.
  - Gain in a term the change does not touch measures noise in the comparison.
- **Lesson 2: check the incumbent arm's draw against its own earlier replicates.**
  - E002 scored 123, 117 and now 116 of 140 on the same dev tasks. E005's 123 only ties E002's best replicate, so part of the lift is a low incumbent draw.
  - The paired SE covers this in expectation, but on a single run the decomposition and the replicate history show where it came from.
- **Lesson 3: identical-prompt positions give a free noise band.**
  - E002's 4 sample positions are exchangeable, and their per-position pass rates spread 0.586 to 0.679 at n = 140.
  - Any per-angle difference inside that band is not a finding. E005's angles stayed in or above it, so they cost nothing per sample, and none clearly helped.
