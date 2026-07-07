# Senior SWE-Bench Retry Chain Smoke

**Date:** 2026-07-06
**Scope:** Retry-controller/status path hardening; not official Senior SWE-Bench mastery.

## Lineage

Previous retry slices made each Senior SWE-Bench retry boundary deterministic: failed retry status exposes a next-cycle gate, next-cycle summaries can build resume-attempt plans, resumed attempts run evaluator/evidence gates, and status validates successful evidence before allowing claims. The remaining gap was an integration smoke proving these handoffs can advance a bounded fixture chain one gate at a time through inspected evidence, while remaining provider-free except for the fixture-represented next-cycle summary.

## Change

Added `retry_run_next_gate_advances_fixture_chain_one_gate_at_a_time_to_inspected_run_result` in `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs`.

The test:

1. Runs an initial precomputed retry attempt that fails and emits a next-cycle boundary.
2. Injects a fixture `a2d.cycle-output-artifacts.v1` manifest for the next attempt, representing the already-bounded provider/cycle boundary.
3. Invokes `senior-swe-bench-retry-run-next-gate --next-cycle-execution ...` once and verifies only a resume-attempt plan is produced.
4. Invokes `senior-swe-bench-retry-run-next-gate --retry-attempt-plan ...` once and verifies the resumed attempt reaches inspected `a2d.fitness-evidence.v1` evidence.
5. Runs `senior-swe-bench-retry-status` from a non-root CWD to prove repo-relative final evidence paths are resolved through retry artifact semantics.

The slice also fixes `build_senior_swe_bench_retry_status` to resolve `final_evidence_path` with `resolve_retry_artifact_path` before reading, so project-relative evidence paths are not interpreted against arbitrary caller CWDs.

## Validation

```bash
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_advances_fixture_chain_one_gate_at_a_time_to_inspected_run_result -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
CARGO_BUILD_JOBS=2 cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-retry-chain-smoke-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260706-retry-chain-smoke-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_scope: crates`, `source_diff_hash: a53e71d5418002600df0cd8c44e92ae7028f5b76`, matching the scoped crates diff.

## Interpretation

This is retry-chain/status path hardening. The provider next-cycle step is represented by a fixture summary, and the evaluator is a local fixture. It is not official Senior SWE-Bench mastery, not OS/network no-search enforcement, and not a live provider-loop proof.
