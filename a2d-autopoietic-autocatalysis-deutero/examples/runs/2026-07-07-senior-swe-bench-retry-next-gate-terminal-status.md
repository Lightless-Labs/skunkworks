# Senior SWE-Bench Retry Next-Gate Terminal Status Coverage

**Date:** 2026-07-07
**Scope:** Retry next-gate terminal-status branch coverage; not official Senior SWE-Bench mastery.

## Lineage

Recent next-gate work covered the one-gate controller paths for running the next cycle, planning a resumed attempt, executing a resumed attempt, preserving no-search metadata, and failing closed before provider invocation on invalid checkout context. Retry status also revalidates terminal run-result boundary booleans before accepting success.

The remaining controller branch gap was terminal observation: once a retry execution is already successful, `senior-swe-bench-retry-run-next-gate --retry-execution <retry-execution.json>` should report that no next gate was executed and should not restart cycle-input, providers, evaluators, or evidence inspection.

## Change

`crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` now extends `retry_run_next_gate_advances_fixture_chain_one_gate_at_a_time_to_inspected_run_result` after retry-status validation:

- invokes `senior-swe-bench-retry-run-next-gate --retry-execution <terminal retry-execution.json>` from `crates/a2d-cli` with a repo-relative path;
- asserts `executed_gate: none_terminal_status` and `stop_reason: terminal_retry_status_no_gate_executed`;
- asserts `child_schema` and `child_artifact_path` are null;
- asserts provider, cycle-input, evaluator, and evidence-inspection side-effect booleans are false;
- asserts `github_solution_search_allowed: false`, `fitness_claim_allowed_after_gate: false`, and `official_senior_swe_bench_mastery: false`;
- verifies the evaluator counter remains unchanged; and
- verifies `retry-next-gate-terminal-status.json` is persisted and equals stdout.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_advances_fixture_chain_one_gate_at_a_time_to_inspected_run_result -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-next-gate-terminal-status-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-next-gate-terminal-status-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `failed_cases: []`, `source_diff_scope: crates`, `source_diff_hash: 36a0969c1aa03003cf35e02391202d0f5374ef79`, matching the scoped crates diff.

## Interpretation

This is retry-controller terminal-status/source-patch hardening only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a live provider-loop success proof.
