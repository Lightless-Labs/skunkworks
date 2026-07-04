# Cycle Input Bridge — 2026-07-04

## Purpose

Make the Senior SWE-Bench task-cycle-input artifact path explicit at the CLI boundary. The existing `input_artifacts_from_request` parser could seed JSON artifact bundles, but `a2d cycle` took a raw argument string and there was no clear file/stdin command for consuming a generated task-cycle-input JSON artifact.

## Lineage constraints

- Senior SWE-Bench task-cycle-input artifacts remain CLI/evaluation-layer plumbing; `a2d-core` stays benchmark-generic.
- Coding agents must still solve from provided context/local checkout/local tests only; this slice does not authorize public solution search.
- Mechanical runtime artifacts (`fitness_report`, `failure_report`, provider health/policy, `system_code`) must not be seedable by a user-provided cycle input, because those are produced by benchmark/runtime mechanisms.
- Evidence for the source patch must be fresh `a2d.fitness-evidence.v1` actual-test evidence; this is not an official Senior SWE-Bench mastery claim.

## Change

Added:

```bash
a2d cycle-input <artifact-bundle.json|-> [cycles]
```

The command reads a JSON object artifact bundle from a file or stdin, rejects non-object input before running providers, rejects reserved runtime artifacts such as `fitness_report` and `failure_report`, then reuses the existing cycle seeding path for `requirements`, `design`, `plan`, `benchmark_context`, and similar task context artifacts.

## Validation

Focused tests:

```bash
cargo fmt --check
cargo test -p a2d cycle_input -- --nocapture
cargo test -p a2d --test cycle_input -- --nocapture
```

Full test suite:

```bash
cargo test
```

Result: 300 passed, 2 ignored.

Independent review found no critical issues and warned that making JSON cycle inputs first-class could expose pre-existing unrestricted seeding of reserved runtime artifacts. The implementation was tightened to reject those artifacts before persistence.

## Binary smokes

Negative smoke: non-JSON stdin is rejected before the cycle starts.

Artifact: `runs/20260704-cycle-input-bridge-evidence/negative-smoke/non-json.err`

Negative smoke: JSON containing reserved `fitness_report` is rejected before the cycle starts.

Artifact: `runs/20260704-cycle-input-bridge-evidence/negative-smoke/reserved.err`

Both commands exited with status 1 and did not print `A²D Catalytic Cycle`.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-cycle-input-bridge-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260704-cycle-input-bridge-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260704-cycle-input-bridge-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass: true`
- `source_diff_scope: crates`
- `source_diff_hash: 918971a1f1be72793a3fb2f6c68dfa9e300c0825`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`

This gates the cycle-input CLI source patch with fresh full-passing Sudoku score-artifact evidence. It does not prove that A²D has solved an official Senior SWE-Bench task.
