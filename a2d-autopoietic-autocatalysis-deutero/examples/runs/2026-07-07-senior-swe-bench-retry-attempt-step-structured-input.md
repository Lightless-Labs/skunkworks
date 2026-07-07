# Senior SWE-Bench Retry Attempt-Step Structured Next-Cycle Input

**Date:** 2026-07-07
**Scope:** Retry feedback structured-context regression coverage; not official Senior SWE-Bench mastery.

## Lineage

The previous retry-attempt-step feedback slice proved failed public local evaluator output reaches `next_cycle_input` without seeding `fitness_report` or `failure_report`. The adjacent risk was serialization drift: a future edit could JSON-stringify `next_cycle_input`, making evidence and status plumbing pass while the next `cycle-input` gate receives an opaque string blob instead of structured retry context.

## Change

`crates/a2d-cli/tests/senior_swe_bench_retry_attempt_step.rs` now asserts the failed-attempt `retry_step.next_cycle_input` is a JSON object and still contains structured retry fields:

- `requirements`
- `design`
- `plan`
- `benchmark_context`
- `evaluation`

It also pins task id, repo, evaluator kind, and previous-plan context so the next cycle retains the retry plan/path-derived context rather than a lossy string.

Production recon found no `next_cycle_input` JSON stringification path; `rg` matches were path strings or status text only.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_step retry_attempt_step_builds_next_cycle_input_for_failed_attempt -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_attempt_step -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-attempt-step-structured-input-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-attempt-step-structured-input-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `source_diff_scope: crates`, `source_diff_hash: bf1253461fb85e666beb7127195ef39bded20396`, matching the scoped crates diff. Senior SWE-Bench official hidden holdouts are not applicable to this local source-patch gate.

## Interpretation

This is retry feedback structured-context test hardening only. It is not official Senior SWE-Bench mastery, network no-egress proof, or live provider/evaluator quality evidence.
