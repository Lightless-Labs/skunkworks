# Senior SWE-Bench Retry Next-Gate Controller

**Date:** 2026-07-06
**Scope:** CLI retry orchestration plumbing; not official Senior SWE-Bench mastery.

## Lineage

Prior retry slices made each boundary machine-verifiable: status, next-cycle execution, resume-attempt planning/execution, evidence inspection, and official evaluator manifest inspection. The remaining controller gap was ergonomic and safety-related: an autonomous runner needed a single command that can execute exactly one audited next gate without turning into an unbounded provider/evaluator loop or bypassing `fitness-evidence-inspect --require-all-tests-pass`.

## Change

Added CLI-only `a2d senior-swe-bench-retry-run-next-gate` in `crates/a2d-cli/src/main.rs`.

Modes:

- `--retry-execution <retry-execution.json>`: reads retry status and, only when status says `run_next_cycle`, executes exactly one persisted `cycle-input` boundary.
- `--next-cycle-execution <retry-next-cycle-execution.json> --retry-plan <retry-plan.json> ... -- <evaluator> [args...]`: builds and persists exactly one resumed attempt plan from a successful next-cycle summary.
- `--retry-attempt-plan <retry-attempt-plan.json>`: executes exactly one resumed deterministic attempt.

The controller emits `a2d.senior-swe-bench-retry-next-gate-execution.v1`, records child schema/path, preserves no-search/no-pre-evidence-fitness metadata, and preflights controller artifacts before child side effects. It does not loop or execute the next emitted gate.

## Validation

Focused:

```bash
cargo fmt --check
cargo test -p a2d retry_run_next_gate -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test -p a2d senior_swe_bench -- --nocapture
```

Full:

```bash
cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-retry-next-gate-controller-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

target/debug/a2d fitness-evidence-inspect \
  runs/20260706-retry-next-gate-controller-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- `source_diff_scope: crates`
- `source_diff_hash: 370d723d342ce205aadb025484b53f2e2d34d19e`
- `hidden_acceptance: not_present` for this Sudoku score-artifact source-patch gate; Senior SWE-Bench official hidden-holdout mastery is not claimed.

## Interpretation

This is a controller/readiness slice toward autonomous Senior SWE-Bench retry orchestration. It is not a public solution-search enforcement proof, not an OS/network isolation proof, and not official Senior SWE-Bench task mastery.
