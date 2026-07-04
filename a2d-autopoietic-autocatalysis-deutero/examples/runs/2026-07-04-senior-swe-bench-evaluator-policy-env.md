# Senior SWE-Bench Evaluator Policy Environment — 2026-07-04

## Purpose

Make the Senior SWE-Bench no-public-solution-search contract mechanically visible to the evaluator subprocess, not only to the task package parser and coding-agent prompt.

## Lineage constraints

- Senior SWE-Bench integration is CLI/evaluation-layer only; no benchmark adapter code belongs in `a2d-core`.
- Foundry-style information barriers require policy to be carried as typed/contextual data across process boundaries.
- Prior Senior SWE-Bench slices already reject task inputs that allow GitHub solution search, bind candidate patch hashes, preflight patch applicability, and distinguish local-wrapper evidence from official benchmark evidence.

## Change

`a2d senior-swe-bench-evaluate` now exports two policy environment variables to the evaluator command:

- `A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED=false`
- `A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN=true`

The values are derived from the already-validated task package or cycle input. Because those inputs are rejected when solution search is allowed, successful evaluations expose an explicit no-search contract to local/official harness wrappers.

## Validation

Focused checks:

```bash
cargo fmt --check
cargo test -p a2d senior_swe_bench -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 293 passed, 2 ignored.

Boundary check:

```bash
rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench|sudoku_solver|rubiks_cube|chess_engine|sudoku-solver|rubiks-cube|chess-engine" crates/a2d-core
```

No matches.

## Senior SWE-Bench local-wrapper smoke

A mock checkout, candidate diff, no-search task package, and evaluator script were stored under `runs/20260704-senior-swe-bench-evaluator-policy-evidence/local-evaluator/`. The evaluator script required patched checkout content, preserved original checkout content, and the two no-search policy environment variables before exiting successfully.

Evidence artifact: `runs/20260704-senior-swe-bench-evaluator-policy-evidence/local-evaluator/fitness/senior-swe-bench-firezone-fix-connlib-align-device-hard-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (3/3)
- `failed_cases: []`
- result labels: `all_tests_pass`, `has_no_solution_search`, `hidden_acceptance`
- `candidate_patch_hash: e61dbc5efcae0fda0dca699dae5c782d69e9e5e3`
- `candidate_patch_applied: true`
- `evaluator_checkout_mode: isolated_copy`
- `original_checkout_mutated: false`
- `candidate_patch_preflight_status: passed`
- `evaluator_kind: provided_local_command`
- `source_diff_hash: c18edad00627bf325fadb84ff65468289e7fe693`

Local evaluation artifact: `runs/20260704-senior-swe-bench-evaluator-policy-evidence/local-evaluator/firezone-fix-connlib-align-device-hard-policy-env-local-evaluation.json` recorded `github_solution_search_allowed: false` and `stdout_preview: senior-swe-bench-policy-env-ok`.

## Source-patch gate evidence

Command:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260704-senior-swe-bench-evaluator-policy-evidence/actual-test-score-artifact \
  cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260704-senior-swe-bench-evaluator-policy-evidence/actual-test-score-artifact/baseline-sudoku-solver-cycle-0-fitness-evidence.json`

Inspected fields:

- `schema_version: a2d.fitness-evidence.v1`
- `actual_tests_evaluated: true`
- `non_regressing: true`
- `fitness: 1.0` (6/6)
- `failed_cases: []`
- result labels include `all_tests_pass`
- `source_diff_scope: crates`
- `source_diff_hash: c18edad00627bf325fadb84ff65468289e7fe693`

This slice strengthens policy propagation and evaluator observability. It is still provided-local-command evidence, not official Senior SWE-Bench task mastery.
