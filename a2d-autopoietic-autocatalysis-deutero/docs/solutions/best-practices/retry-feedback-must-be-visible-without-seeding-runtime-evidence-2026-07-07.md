---
module: a2d-cli
tags: [senior-swe-bench, retry-feedback, evidence-gates, no-search-policy]
problem_type: best-practice
---

# Retry feedback must be visible without seeding runtime evidence

## Problem

Failed evaluator output is useful food for the next coder attempt, but it must not become trusted runtime evidence. If retry feedback injects `fitness_report` or `failure_report`, or allows public solution references into visible feedback, the next attempt can bypass the evidence gate or violate Senior SWE-Bench no-public-solution-search constraints.

## Pattern

For failed non-final Senior SWE-Bench retry attempts:

- carry only declared public local-test feedback into the next cycle input;
- keep `evaluation.status: not_evaluated` and `evaluation.fitness: null`;
- preserve `github_solution_search_allowed: false`;
- do not seed reserved runtime artifacts such as `fitness_report` or `failure_report`;
- reject GitHub/public solution references before the feedback becomes coder-visible.

The retry-step command should be deterministic plumbing only: it starts no provider/evaluator and makes no fitness claim.

## Evidence

Regression coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step.rs` proves `senior-swe-bench-retry-attempt-step` wires failed public local evaluator output into `next_cycle_input` while preserving no-search and no-runtime-evidence boundaries, and rejects visible feedback containing a GitHub PR URL.

Fresh source-patch evidence: `runs/20260707-retry-attempt-step-feedback-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`, full-passing `a2d.fitness-evidence.v1` with `source_diff_hash: 0c4de0c856af01ea268842d12b6a89d58187bfaf` matching the scoped crates diff.
