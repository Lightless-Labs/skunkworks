# Senior SWE-Bench Retry Attempt-Step Feedback Wiring

**Date:** 2026-07-07
**Scope:** Retry feedback integration coverage; not official Senior SWE-Bench mastery.

## Lineage

A²D already had safe feedback construction in `build_senior_swe_bench_cycle_input_feedback` and retry-step execution through `senior-swe-bench-retry-attempt-step`. Recent retry-chain hardening proved one-gate execution boundaries, but the failed-evaluator feedback path needed an integration regression showing the retry-attempt-step command actually carries public local evaluator feedback into the next cycle input without seeding runtime evidence artifacts.

## Change

`crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step.rs` now strengthens failed-attempt coverage:

- the failed local-evaluator fixture includes public local stdout/stderr feedback;
- `retry_attempt_step_builds_next_cycle_input_for_failed_attempt` asserts the resulting `next_cycle_input` is still `not_evaluated`, has `fitness: null`, keeps `github_solution_search_allowed: false`, does not seed `fitness_report` or `failure_report`, and contains the public local feedback plus no-search warning in design text;
- `retry_attempt_step_rejects_public_solution_reference_in_visible_feedback` tampers visible evaluator feedback with a GitHub PR URL and proves `senior-swe-bench-retry-attempt-step` fails closed before returning coder-visible feedback.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_step retry_attempt_step -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Reviewer subagent found no blockers/warnings and independently ran the retry-attempt-step integration suite (6/6 passed).

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-attempt-step-feedback-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-attempt-step-feedback-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, `source_diff_scope: crates`, `source_diff_hash: 0c4de0c856af01ea268842d12b6a89d58187bfaf`, matching the scoped crates diff. The source-patch gate has aggregate `all_tests_pass: true`; Senior SWE-Bench official hidden holdouts are not applicable to this local test-coverage slice.

## Interpretation

This is retry feedback integration test hardening only. It does not prove official Senior SWE-Bench mastery, network no-egress, or live provider/evaluator quality.
