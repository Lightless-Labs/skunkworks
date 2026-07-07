# Senior SWE-Bench Retry Resume-Execute Controller Preflight Coverage

**Date:** 2026-07-07
**Scope:** Retry next-gate resume-execute controller artifact collision coverage; not official Senior SWE-Bench mastery.

## Lineage

After `7465bb3` hardened terminal next-gate artifact preflight, scout recon found a symmetric controller branch that still lacked adversarial coverage: `senior-swe-bench-retry-run-next-gate --retry-attempt-plan ...` already preflighted `attempt-N/retry-next-gate-resume-execute.json`, but only the happy fixture-chain path covered that branch. The terminal branch had a stale-controller-artifact regression; the resume-execute branch did not.

Prior retry-loop learnings constrain this slice:

- retry gates must execute exactly one bounded step and stop before side effects when preflight fails;
- stale controller artifacts should be rejected by explicit next-gate preflight, not by generic JSON overwrite protection; and
- local-wrapper/controller evidence must not be described as official Senior SWE-Bench mastery or no-egress proof.

## Change

`crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` adds `retry_run_next_gate_rejects_existing_resume_execute_controller_artifact_before_overwrite`.

The regression builds a failed first attempt, plans a resumed attempt, writes the plan to `attempt-1/retry-attempt-plan.json`, plants stale bytes at `attempt-1/retry-next-gate-resume-execute.json`, and invokes:

```bash
a2d senior-swe-bench-retry-run-next-gate --retry-attempt-plan <attempt-1/retry-attempt-plan.json>
```

It asserts exit `1`, stderr contains `already exists before child side effects`, the stale controller artifact is unchanged, the evaluator counter remains at the first-attempt value, and the terminal `retry-resume-attempt-execution.json` summary is not persisted.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_rejects_existing_resume_execute_controller_artifact_before_overwrite -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-resume-execute-controller-preflight-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-resume-execute-controller-preflight-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present` for this score-artifact source-patch gate, `source_diff_scope: crates`, `source_diff_hash: 8382736936a397397d6aca7f45caf6fb9516236f`, matching the scoped crates diff.

## Interpretation

This is retry next-gate resume-execute controller artifact preflight coverage only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not live provider-loop success evidence.
