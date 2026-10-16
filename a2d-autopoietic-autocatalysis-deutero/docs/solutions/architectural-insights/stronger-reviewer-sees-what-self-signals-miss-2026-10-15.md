---
title: "A Stronger Reviewer Sees What the Coder's Own Signals Miss"
date: 2026-10-15
category: architectural-insights
module: a2d-lift / nano
problem_type: research_finding
component: harness-design
symptoms:
  - "Repair loop saturates: public tests are green on ~99% of tasks, remaining failures are invisible spec misreadings"
  - "Several samples contain a correct file, but no self-generated signal can tell which"
root_cause: same_model_signals_share_the_coders_blind_spots
---

# A Stronger Reviewer Sees What the Coder's Own Signals Miss

**Evidence:** LOG C2-E1 (offline, the frozen S0 pool of c1-train candidates), against cycle 1's S1–S3.

## The setup

- Luna at low effort, given 4 independent samples per task: at least one sample passes every hidden test on 85% of tasks.
- The plain repair loop's pick passes only 74%.
- Every self-generated signal failed to close that gap:
  - Luna's own spec tests: AUC 0.63;
  - Luna reviewing its own code clause by clause: conversion 0.19;
  - majority agreement among Luna's samples: 0.73.
- The errors are shared misreadings, so the same model cannot see them.

## The finding

- On the 39 tasks where Luna's public-green candidates disagreed on hidden tests, a judge read the spec and the candidates, never the hidden tests, and picked one.
- A Claude judge (Sonnet) picked a passing file 37 times.
- Codex `gpt-6.1-sol` picked one all 39 times, with every command audited: `cat`, `ls` and `rg` inside its own directory, and nothing executed.
- That moves the 4-sample-plus-judge pick rate to 0.83–0.85, against the repair loop's 0.74 at the same 5-call cap.
- The judges' reasons are clause-level and concrete:
  - "validates the host before the port as R3 requires";
  - "Vec::with_capacity(count) panics for huge counts".

## Lessons
- **A harness can only add information or compute.** Once the oracle-visible signal is spent, the next lever is a *different reader*: a stronger or different model, not more of the same model.
- **Measure the selection headroom first.** "Any-pass among k samples" minus "the baseline's pick" bounds what selection can buy. Here it was about 11 pp, and a judge recovered nearly all of it offline.
- **Audit an agentic judge's tool use before trusting a perfect score.** Codex's read-only sandbox does not block reads, and an answer key one directory up would have been one `ls ..` away.
- **Live, the judge needs no tools at all.** Pass the spec and the candidates inline, through a no-tools lane.
- **Claim the right thing.** Sol is a stronger model from the same vendor. The honest claim is "Luna generation plus Sol selection", not "cross-family review". A Claude-lane replication would be needed for a family claim.
- **Equal calls are not equal compute.** A judge call reads 3–5 files. Report tokens per lane beside any invocation-capped result.
- **Follow-up (2026-10-16):** live, the judge reached the ceiling; see `selector-at-ceiling-moves-bottleneck-to-generation-2026-10-16.md`.
