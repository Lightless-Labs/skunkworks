# Senior SWE-Bench Retry Next-Gate Policy Metadata Surfacing

**Date:** 2026-07-07
**Scope:** Retry next-gate controller metadata transparency; not official Senior SWE-Bench mastery.

## Lineage

Recent retry-boundary work made no-public-solution-search policy observable and rejected unsafe next-cycle command boundaries. The remaining controller metadata gap was narrower: `retry_next_gate_execution_value` always emitted `github_solution_search_allowed: false`, so an unsafe or corrupted `before_status`/child artifact could be laundered to safe-looking controller metadata.

## Change

`crates/a2d-cli/src/main.rs` now OR-preserves `github_solution_search_allowed` from the controller `before_status` and child gate artifact. Non-terminal next-gate controller call sites persist through `write_retry_next_gate_execution_artifact`, so the production artifact path writes the same surfaced metadata. The regression covers:

- safe `false` inputs remain `false`;
- `before_status.github_solution_search_allowed: true` is preserved;
- child `github_solution_search_allowed: true` is preserved;
- the controller artifact writer persists the unsafe/tainted value.

This is metadata transparency, not permission to search. Existing no-search task parsing, retry-boundary rejection, provider env propagation, and scoped evidence labels remain separate gates.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_next_gate_execution_surfaces_github_solution_search_policy_from_inputs -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d retry_run_next_gate -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-next-gate-policy-surfacing-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-next-gate-policy-surfacing-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `source_diff_scope: crates`, `source_diff_hash: d2e57b7f0b565118116999e97d78e9b1e2c9f43d`, matching the scoped crates diff.

## Interpretation

This is retry-controller metadata/source-patch hardening only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a provider/evaluator run.
