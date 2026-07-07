# Senior SWE-Bench Retry Status Terminal Run-Result Boundary Validation

**Date:** 2026-07-07
**Scope:** Retry status verifier hardening; not official Senior SWE-Bench mastery.

## Lineage

Retry status already re-read the final `a2d.fitness-evidence.v1` and compared the terminal evidence summary, evaluator kind, and official-mastery fields. After the next-gate metadata work, the remaining status-verifier gap was that boolean claim-boundary fields inside `terminal_run_result` were not independently revalidated before status accepted a successful retry summary.

## Change

`crates/a2d-cli/src/main.rs` now requires successful retry `terminal_run_result` artifacts to preserve reviewed boundary booleans:

- `github_solution_search_allowed: false`
- `fitness_claim_allowed_before_evidence: false`
- `provider_invocations_started: false`
- `evaluator_invocations_started: false`
- `fitness_evidence_inspection_started: true`
- `fitness_evidence_inspection_passed: true`
- `fitness_claim_allowed_after_evidence_inspection: true`

Regression coverage in `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs` tampers successful retry executions and proves status rejects:

- `terminal_run_result.github_solution_search_allowed: true`
- `terminal_run_result.provider_invocations_started: true`
- non-boolean `terminal_run_result.fitness_evidence_inspection_passed`

This keeps status read-only and avoids turning a terminal summary into a source of unchecked claims.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_status_rejects_terminal_run_result -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260707-retry-status-terminal-run-result-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260707-retry-status-terminal-run-result-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `passed: 6`, `total: 6`, `source_diff_scope: crates`, `source_diff_hash: afae99ef0c9a88073a4af6ea1dbd4c7880bc577e`, matching the scoped crates diff. The score-artifact source-patch gate includes the aggregate `all_tests_pass` result; hidden-specific Senior SWE-Bench holdouts are not applicable to this local source-patch gate.

## Interpretation

This is retry-status verifier/source-patch hardening only. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a provider/evaluator live benchmark run.
