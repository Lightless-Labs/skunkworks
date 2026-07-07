---
module: a2d-cli
tags: [senior-swe-bench, retry-feedback, structured-artifacts, evidence-gates]
problem_type: best-practice
---

# Retry next-cycle input must remain structured JSON

## Problem

`next_cycle_input` is the handoff from a failed Senior SWE-Bench retry attempt back into `a2d cycle-input`. If it is accidentally serialized as a JSON string blob, retry status/evidence plumbing can still look successful while the next cycle loses structured task context such as requirements, plan, benchmark metadata, and evaluation state.

## Pattern

Regression coverage should assert that retry feedback produces a JSON object, not just a present value. The object should retain the fields the next cycle consumes:

- `requirements`
- `design`
- `plan`
- `benchmark_context`
- `evaluation`

It should also preserve task/repo/evaluator context and previous-plan context so the next cycle remains a structured retry attempt rather than a lossy text payload.

## Evidence

`crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step.rs` now asserts `retry_step.next_cycle_input.as_object()` and pins the expected structured fields plus task id, repo, evaluator kind, and previous plan text.

Fresh source-patch evidence: `runs/20260707-retry-attempt-step-structured-input-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: bf1253461fb85e666beb7127195ef39bded20396` matching the scoped crates diff.
