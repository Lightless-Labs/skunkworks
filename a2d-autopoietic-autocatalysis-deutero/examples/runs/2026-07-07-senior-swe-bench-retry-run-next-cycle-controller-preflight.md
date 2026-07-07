# Senior SWE-Bench Retry Run-Next-Cycle Controller Preflight Coverage

**Date:** 2026-07-07
**Scope:** Retry next-gate run-next-cycle controller artifact collision coverage; not official Senior SWE-Bench mastery.

## Lineage

The one-gate controller already preflights controller artifacts before child side effects. Recent slices added adversarial coverage for terminal-status and resume-execute controller collisions. The remaining symmetric controller branch was `senior-swe-bench-retry-run-next-gate --retry-execution ...` when retry status says `next_action: run_next_cycle`: it preflighted `attempt-N/retry-next-gate-run-next-cycle.json`, but CLI coverage only proved the happy path.

Prior retry-loop learnings constrain this slice:

- controller preflight must stop before child side effects;
- stale controller artifacts must be rejected by explicit next-gate preflight rather than generic JSON overwrite protection; and
- controller/source-patch evidence is not official Senior SWE-Bench mastery or no-egress proof.

## Change

`crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` adds `retry_run_next_gate_rejects_existing_run_next_cycle_controller_artifact_before_cycle_input`.

The regression creates a failed first retry execution, plants stale bytes at `attempt-1/retry-next-gate-run-next-cycle.json`, and invokes:

```bash
a2d senior-swe-bench-retry-run-next-gate --retry-execution <retry-execution.json>
```

It asserts exit `1`, stderr contains `already exists before child side effects`, the stale controller artifact is unchanged, `retry-next-cycle-execution.json` is not persisted, and no `cycle-output-artifacts/manifest.json` appears. This proves the run-next-cycle branch stops before the `cycle-input` child boundary can produce artifacts.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_rejects_existing_run_next_cycle_controller_artifact_before_cycle_input -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-run-next-cycle-controller-preflight-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-run-next-cycle-controller-preflight-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present` for this score-artifact source-patch gate, `source_diff_scope: crates`, `source_diff_hash: 48f64bbcb20ca720e0bb7f2bd20c3b3f771d0dd1`, matching the scoped crates diff.

## Interpretation

This is retry next-gate run-next-cycle controller artifact preflight coverage only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not live provider-loop success evidence.
