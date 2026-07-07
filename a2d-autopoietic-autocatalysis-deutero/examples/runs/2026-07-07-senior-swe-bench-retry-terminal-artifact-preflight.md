# Senior SWE-Bench Retry Terminal Artifact Preflight Hardening

**Date:** 2026-07-07
**Scope:** Retry terminal/controller artifact overwrite preflight; not official Senior SWE-Bench mastery.

## Lineage

The retry executor and next-gate controller already use no-overwrite artifact writes for many paths, and the previous next-gate terminal-status slice proved a completed retry execution is observed without starting a new gate. A follow-up review found two remaining collision risks worth making explicit:

- the terminal-status controller branch built its terminal observation value before discovering a stale `retry-next-gate-terminal-status.json` collision via generic JSON artifact writing; and
- resumed success writes `attempt-N/retry-run-result.json`, which should be covered by the same pre-evaluator planned-output preflight invariant.

These are controller/audit-trail hardening gaps only. They do not run providers, prove official holdouts, or make a Senior SWE-Bench success claim.

## Change

`crates/a2d-cli/src/main.rs` now runs `preflight_retry_next_gate_output` before constructing/writing the terminal next-gate status artifact.

`crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` adds two regressions:

- `retry_run_next_gate_rejects_existing_terminal_controller_artifact_before_overwrite` creates a successful retry execution, plants a stale `retry-next-gate-terminal-status.json`, then proves `senior-swe-bench-retry-run-next-gate --retry-execution ...` rejects with the next-gate preflight error, preserves the stale artifact, and does not rerun evaluator side effects.
- `retry_resume_attempt_execute_rejects_existing_run_result_before_evaluator` plants a stale resumed `retry-run-result.json` and proves resumed attempt execution rejects through planned-output preflight before rerunning the evaluator.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_rejects_existing_terminal_controller_artifact_before_overwrite -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_resume_attempt_execute_rejects_existing_run_result_before_evaluator -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-terminal-artifact-preflight-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-terminal-artifact-preflight-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, aggregate `all_tests_pass: true`, `hidden_acceptance: not_present` for this score-artifact source-patch gate, `source_diff_scope: crates`, `source_diff_hash: 94db5c21e0f931cc073d222ffc392f8813f40c3a`, matching the scoped crates diff.

## Interpretation

This is retry terminal/controller artifact preflight hardening only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not live provider-loop success evidence.
