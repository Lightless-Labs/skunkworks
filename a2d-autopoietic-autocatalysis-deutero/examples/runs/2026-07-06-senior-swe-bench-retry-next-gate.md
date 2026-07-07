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

## Follow-up test hardening

A follow-up integration test now covers the controller's `--next-cycle-execution ... --retry-plan ...` mode from `crates/a2d-cli/tests/senior_swe_bench_retry_execute.rs`. The test drives a failed first retry attempt, fabricates a successful next-cycle summary/manifest, runs `senior-swe-bench-retry-run-next-gate`, and asserts it produces exactly a resume-attempt plan plus controller artifact without evaluator/evidence-inspection side effects. A root-scoped evaluator counter proves the next-gate planning invocation did not run the evaluator after the setup attempt.

Follow-up validation:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_plans_from_successful_next_cycle_summary_without_evaluator -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test -p a2d retry_run_next_gate -- --nocapture
cargo test
```

Follow-up source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-retry-next-gate-resume-plan-test-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

target/debug/a2d fitness-evidence-inspect \
  runs/20260706-retry-next-gate-resume-plan-test-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: full-passing `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `failed_cases: []`, `source_diff_scope: crates`, `source_diff_hash: 435fb525c24c4f0d0fa5d9e60318f9de9de2a0c6`, matching the scoped crates diff. This remains source-patch/test coverage evidence only; no official Senior SWE-Bench mastery is claimed.

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

## Follow-up: path-normalization hardening

A follow-up slice made retry status/next-cycle handoff paths CWD-stable for in-project artifacts. Retry status, next-cycle-command/summary, and resume-boundary paths now serialize repo-relative strings where possible, resolve repo-relative retry artifacts against the A²D project root, and launch the next-cycle child from the project root so the persisted command text is stable from subdirectories.

Validation:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test -p a2d retry_run_next_gate -- --nocapture
cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-retry-path-normalization-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260706-retry-path-normalization-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: 5b72e098273f7afdc747bf821ba46590ef4f4e55`, matching the scoped crates diff. This remains retry status/next-cycle handoff portability source-patch evidence, not official Senior SWE-Bench mastery or a no-egress proof.

## Follow-up: next-gate CWD-stability regression

A second follow-up covered the controller's successful-next-cycle-summary branch from a non-root CWD. The regression now invokes `senior-swe-bench-retry-run-next-gate --next-cycle-execution <repo-relative> --retry-plan <repo-relative> ...` from `crates/a2d-cli`, asserts `before_status.next_cycle_execution_path` remains repo-relative, and checks the child `resume_boundary` has no embedded project/fixture absolute path prefixes.

Validation:

```bash
cargo fmt --check
cargo test -p a2d --test senior_swe_bench_retry_execute retry_run_next_gate_plans_from_successful_next_cycle_summary_without_evaluator -- --nocapture
cargo test -p a2d --test senior_swe_bench_retry_execute -- --nocapture
cargo test -p a2d retry_run_next_gate -- --nocapture
cargo test
```

Fresh source-patch evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260706-retry-next-gate-cwd-stability-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260706-retry-next-gate-cwd-stability-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: 4c09ac3db7c7553e131c4d76000ac329b4608643`, matching the scoped crates diff. `hidden_acceptance` is `not_present` for this Sudoku source-patch gate. This remains retry controller portability/source-patch evidence, not official Senior SWE-Bench mastery or a no-egress proof.

## Follow-up: complete fixture-chain smoke to inspected evidence

A later slice added `retry_run_next_gate_advances_fixture_chain_one_gate_at_a_time_to_inspected_run_result`, which drives the persisted retry chain from failed precomputed attempt through fixture next-cycle manifest, resume-plan next-gate, resumed-execute next-gate, final inspected evidence, and retry-status validation from `crates/a2d-cli`. The slice also fixes retry status to resolve `final_evidence_path` with retry artifact semantics before reading, keeping project-relative final evidence paths CWD-stable.

Validation:

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

Evidence summary: `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `fitness: 1.0`, `failed_cases: []`, `source_diff_hash: a53e71d5418002600df0cd8c44e92ae7028f5b76`, matching the scoped crates diff. This remains retry-chain/status path hardening; the next-cycle provider step is fixture-represented, and no official Senior SWE-Bench mastery is claimed.
