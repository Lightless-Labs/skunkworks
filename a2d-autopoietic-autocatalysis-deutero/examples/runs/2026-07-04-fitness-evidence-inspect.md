# Fitness Evidence Inspect CLI — 2026-07-04

## Purpose

Add a first-class CLI inspection path for exported `a2d.fitness-evidence.v1` artifacts so evidence review is no longer ad hoc `jq`/manual JSON reading before persistence decisions.

## Lineage constraints

- Prior evidence-export work established `a2d.fitness-evidence.v1` as the durability gate for source patches, mutations, provider-policy changes, and self-improvement.
- Source provenance validation must stay centralized: inspected evidence must pass the existing exported-evidence validator, including `source_diff_scope: crates` and `source_diff_hash` matching the current crates diff.
- Partial non-regressing evidence is useful for debugging and prompt compliance, but benchmark mastery and gate promotion need aggregate acceptance (`all_tests_pass`) and consistent zero-failure totals.
- Hidden details remain protected: the CLI prints aggregate status only and does not reveal hidden holdout case names.

## Change

Added:

```bash
a2d fitness-evidence-inspect <evidence.json> [--require-all-tests-pass]
```

The command validates current exported evidence, requires `actual_tests_evaluated: true` and `non_regressing: true`, prints reviewed summary fields, and optionally requires a consistent all-tests-pass state: `all_tests_pass: true`, `failed == 0`, `passed == total`, and no failed result entries.

## Validation

Focused tests:

```bash
cargo fmt --check
cargo test -p a2d fitness_evidence_inspect_requires_current_non_regressing_actual_tests -- --nocapture
cargo test -p a2d --test score_artifact score_artifact_exports_fitness_evidence_before_nonzero_exit -- --nocapture
```

Full test suite:

```bash
cargo test
```

Result: 296 passed, 2 ignored.

Independent review found one critical issue in the first implementation: `--require-all-tests-pass` accepted any passing `all_tests_pass` result even if other cases failed. The implementation was tightened and covered with contradictory-evidence regression coverage before persistence.

## Fresh fitness evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-fitness-evidence-inspect-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs

cargo run -q -p a2d -- fitness-evidence-inspect \
  runs/20260704-fitness-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json \
  --require-all-tests-pass
```

Artifact: `runs/20260704-fitness-evidence-inspect-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0`
- `failed_cases: []`
- result labels include `all_tests_pass: true`
- `source_diff_scope: crates`
- `source_diff_hash: 5370681c12650e4236e4fb1bcc2cc4600ebb4794`, matching `git diff --binary HEAD -- crates | git hash-object --stdin`

This gates the CLI inspection source patch with fresh full-passing Sudoku score-artifact evidence. It is not a new claim of repeated Sudoku mastery or official Senior SWE-Bench mastery.
