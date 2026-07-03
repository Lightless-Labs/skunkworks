# Senior SWE-Bench Audit Adapter — 2026-07-03

## Purpose

Bootstrap Senior SWE-Bench as an external benchmark source without putting challenge-specific catalog logic in `a2d-core`. The adapter lives in `crates/a2d-cli/src/senior_swe_bench.rs` and exposes a CLI audit/task-context command; `a2d-core` has no `senior_swe_bench` references.

## Lineage constraints

- Foundry/holdout pattern: benchmark task context must preserve structural information barriers.
- Current objective: use Senior SWE-Bench as the benchmark, while forbidding coding agents from searching GitHub for solutions.
- Existing A²D evidence invariant: this is only catalog/audit plumbing; official Senior SWE-Bench evaluation remains required for task fitness. The smoke evidence below gates the source patch as non-regressing, not benchmark mastery.

## Change

- Added private CLI module `senior_swe_bench` for parsing the public Next.js/RSC task list.
- Added `a2d senior-swe-bench-audit <html|->` to emit `a2d.senior-swe-bench-audit.v1` catalog summaries.
- Added `a2d senior-swe-bench-audit <html|-> task-context <task-id>` to render task context with an explicit no-GitHub-solution-search preamble.
- Kept Senior SWE-Bench out of `a2d-core`; this is an external benchmark adapter, not core challenge physics.

## Validation

```bash
cargo test -p a2d senior_swe_bench -- --nocapture
cargo test -p a2d-core senior_swe_bench -- --nocapture  # confirms no core tests/references
cargo test
```

Full suite result: 259 passed, 2 ignored.

Live catalog audit against `https://senior-swe-bench.snorkel.ai/tasks`:

```bash
cargo run -q -p a2d -- senior-swe-bench-audit /tmp/senior-swe-bench-tasks-20260703.html \
  > /tmp/a2d-senior-swe-bench-audit-20260703.json
```

Tracked audit copy: `runs/20260703-senior-swe-bench-audit-evidence/audit/senior-swe-bench-audit.json`.

Observed catalog: 50 benchmark tasks, 10 sample tasks, 12 repositories, task types `{bug: 20, feature: 24, migration: 1, performance: 5}`. The audit explicitly records `github_solution_search_allowed: false`.

## Fresh fitness evidence

Source-patch gate:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-audit-evidence/challenge-smoke \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 1
```

Artifact: `runs/20260703-senior-swe-bench-audit-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`.

Support replay evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-senior-swe-bench-audit-evidence/baseline-good \
cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260703-senior-swe-bench-audit-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Both artifacts are `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, and `non_regressing: true`, with `source_diff_hash: f2e2282d52631f75747a3ae69ba7b46bf75b8720`. The live challenge smoke reached 67% (4/6), so it is non-regressing source-patch evidence only; the saved-artifact replay is full-passing support evidence for the evidence/export path, not autonomous Senior SWE-Bench performance.
