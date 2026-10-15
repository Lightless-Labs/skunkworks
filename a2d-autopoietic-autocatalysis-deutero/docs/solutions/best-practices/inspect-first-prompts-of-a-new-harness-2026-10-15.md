---
title: "Inspect the First Prompts of a New Harness Before Trusting Its Run"
date: 2026-10-15
category: best-practices
module: a2d-cli/lift_arm_c
problem_type: process_lesson
component: harness-design
symptoms:
  - "A new harness configuration underperforms, and the result reads like a verdict on the mechanism being tested"
  - "Arm C fails to reach public-green more often than the plain repair loop, even though it has the same budget"
root_cause: implicit_contracts_of_the_machinery_behind_a_harness_slot
---

# Inspect the First Prompts of a New Harness Before Trusting Its Run

## What happened

The harness-as-data slots for arm C (`coder_prompt`, `coder_note`, `lessons`, …) sit on top of the seed A²D metabolism. That machinery has **implicit contracts** that no slot mentions. Two of them broke results in one day:

1. **The `main()` clause (E08, aborted).**
   - The metabolism's internal sandbox compiles the candidate *as a program* before it runs the public tests. The seed coder prompt said "contain a main() function".
   - A cleaned prompt dropped that line. Every first attempt then failed with a spurious `E0601: main function not found`, the second call was spent adding `fn main`, and the real public-test feedback never reached the coder.
   - It was caught by reading the second coder prompt's `failure_report`, 23 tuples into the run.
2. **Triplicated input (S4).**
   - The seed copies the task input into `requirements`, `design` *and* `plan`, so every C coder prompt carried the spec, the public tests and any appended note three times: 43K characters against D's 15K.
   - An "oracle lessons" ceiling test came in below its threshold. The arm-level breakdown showed C reaching public-green less often (8 vs 2 failures), which first looked like a budget effect.
   - Checking D's first-green attempt index refuted the budget story. Counting the note's occurrences per prompt (3 in 109/109) found the cause.

Neither defect was a finding about the mechanism under test. Both were plumbing.

## The practice

- **Before trusting a new harness's run, read the first two or three prompts its coder receives.** Check that each input appears once, that feedback is the feedback you expect (public-test output, not a compiler complaint about scaffolding), and that the prompt size is comparable with the baseline arm's.
- **Pre-register an early check** in the LOG entry ("the first five C tasks' second prompts carry public-test output, no E0601; otherwise abort") and run it a few minutes in.
- **Encode each contract you discover as validation**, not as a note. `validate_harness` now rejects a `coder_prompt` without `main()`, and `dedupe_input` has a unit test asserting that the spec appears once per coder prompt. P4's proposer edits these slots, so a contract that lives only in prose will be broken again.
- **On metric-only data, diagnose from aggregate counts.** Note occurrences per prompt, mean prompt length and per-arm green rates are counts that need no task content. That keeps the diagnosis inside the validation boundary.
- **A failed pre-registered rule stands.** Record the diagnosis as an observation, fix the defect as a floor change to the baseline, and test the corrected mechanism as the plan's single allowed revision, on data the mechanism was not derived from.
