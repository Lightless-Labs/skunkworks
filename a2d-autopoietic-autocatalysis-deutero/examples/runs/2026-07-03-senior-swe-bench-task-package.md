# Senior SWE-Bench Task Package Boundary — 2026-07-03

## Purpose

Turn the Senior SWE-Bench catalog adapter into a typed task-package source while preserving the architecture boundary: Senior SWE-Bench parsing and schemas remain in `crates/a2d-cli`, and `a2d-core` stays limited to generic benchmark/evidence primitives.

## Lineage constraints

- Prior Senior SWE-Bench audit work proved catalog visibility only; it did not create an evaluated benchmark run.
- Foundry-style information barriers require coding-agent context that forbids public solution search.
- The evidence invariant requires a fresh non-regressing `a2d.fitness-evidence.v1` source-patch gate before persisting source changes.

## Change

- Added `a2d senior-swe-bench-audit <html|-> task-package <task-id>`.
- The package schema is `a2d.senior-swe-bench-task-package.v1`.
- Packages include catalog provenance (`in_benchmark`, `in_sample`, `tags`), no-GitHub restrictions, rendered coding context, and structured evaluation state:
  - `status: not_evaluated`
  - `evaluator: official_senior_swe_bench`
  - `fitness: null`
- Added a regression test that scans `crates/a2d-core/src` and fails if Senior SWE-Bench adapter text re-enters core.

## Validation

```bash
cargo fmt --check
cargo test
```

Full suite result: 268 passed, 2 ignored.

Task-package smoke:

```bash
cargo run -q -p a2d -- \
  senior-swe-bench-audit /tmp/senior-swe-bench-tasks-20260703.html \
  task-package firezone-fix-connlib-align-device-hard \
  > runs/20260703-senior-swe-bench-task-package-evidence/task-package/firezone-fix-connlib-align-device-hard-package.json
```

Validated fields: schema version, task id, `github_solution_search_allowed: false`, "Do not search GitHub" context, `evaluation.status: not_evaluated`, `evaluation.fitness: null`, `in_benchmark: true`, `in_sample: false`.

Architecture-boundary check:

```bash
rg -n "senior_swe_bench|SeniorSweBench|Senior SWE-Bench|senior-swe-bench" \
  crates/a2d-core crates/a2d-cli Cargo.toml
```

All matches are in `crates/a2d-cli`; `crates/a2d-core` has no matches.

## Fresh fitness evidence

Source-patch gate:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-task-package-evidence/challenge-smoke \
A2D_PROVIDER_TIMEOUT_SECS=120 \
A2D_MAX_CYCLE_SECS=180 \
cargo run -q -p a2d -- challenge sudoku 1
```

Artifact: `runs/20260703-senior-swe-bench-task-package-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`.

Support replay evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-task-package-evidence/baseline-good \
cargo run -q -p a2d -- score-artifact sudoku \
  runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260703-senior-swe-bench-task-package-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Both evidence artifacts are `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `source_revision: 65d25fc`, and `source_diff_hash: 51d0e6f5fd1a74c05d827f69a55900d4b3aeea9b`. The live challenge smoke reached 83% (5/6) with failed case `all_tests_pass`, so it gates this source patch as non-regressing only. The saved-artifact replay is full-passing support evidence, not proof that A²D solves Senior SWE-Bench.

## Status

This slice improves packaging and boundary enforcement only. The package is explicitly `not_evaluated`; actual Senior SWE-Bench solving/evaluation still requires the next evidence-backed gap: a repo checkout/harness/official-evaluator + hidden-holdout runner that preserves the no-public-solution-search rule.
