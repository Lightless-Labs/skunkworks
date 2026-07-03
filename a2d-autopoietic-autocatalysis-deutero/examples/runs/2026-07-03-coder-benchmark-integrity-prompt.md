# Coder Benchmark Integrity Prompt — 2026-07-03

## Purpose

Make the Senior SWE-Bench no-solution-search policy visible to the coding enzyme, not only to the catalog audit command. This is a generic benchmark-integrity rule in the seed coder prompt: when requirements/design/plan forbid GitHub, issues, PRs, commits, forks, public web pages, or solution writeups for benchmark answers, the coder must solve from provided context and local tests only.

## Lineage constraints

- Senior SWE-Bench objective: coding agents must not search GitHub for solutions.
- Foundry information-barrier pattern: the benchmark prompt must explicitly preserve the solution-search boundary.
- A²D evidence invariant: prompt normalization changes behavior and therefore require fresh non-regressing `a2d.fitness-evidence.v1` evidence before persistence.

## Change

- Added `coder_benchmark_integrity_rule()` to `crates/a2d-cli/src/main.rs`.
- Inserted that rule into the seed coder prompt.
- Updated `normalize_loaded_enzymes()` so legacy coder prompts are upgraded, while evolved prompts that already mention `design` and `plan` are preserved and only have the integrity rule appended.
- Added regression coverage to ensure evolved prompt hints are not silently wiped during normalization.

This remains generic CLI germline policy, not Senior SWE-Bench code in `a2d-core`; `a2d-core` still has no `senior_swe_bench` references.

## Validation

Focused tests:

```bash
cargo test -p a2d normalize_loaded_enzymes -- --nocapture
cargo test -p a2d coder -- --nocapture
```

Full suite:

```bash
cargo test
```

Result: 260 passed, 2 ignored.

## Fresh fitness evidence

Source-patch gate:

```bash
A2D_GERMLINE=seed \
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-coder-benchmark-integrity-evidence/challenge-smoke \
A2D_PROVIDER_TIMEOUT_SECS=90 \
A2D_MAX_CYCLE_SECS=120 \
cargo run -p a2d -- challenge sudoku 1
```

Artifact: `runs/20260703-coder-benchmark-integrity-evidence/challenge-smoke/sudoku-solver-cycle-0-fitness-evidence.json`.

Support replay evidence:

```bash
A2D_FITNESS_EVIDENCE_EXPORT_DIR=runs/20260703-coder-benchmark-integrity-evidence/baseline-good \
cargo run -q -p a2d -- score-artifact sudoku runs/20260701-score-artifact-fitness-evidence/good-sudoku-artifact.rs
```

Artifact: `runs/20260703-coder-benchmark-integrity-evidence/baseline-good/baseline-sudoku-solver-cycle-0-fitness-evidence.json`.

Both artifacts are `a2d.fitness-evidence.v1`, `actual_tests_evaluated: true`, `non_regressing: true`, `source_revision: 2f88a93` (the current `crates` tree at base HEAD), `source_tree_dirty: true`, and `source_diff_hash: 9916603b8d352a3316b9e1964392693f33fa41ec`, which matches the current `crates` diff at evidence time. The live challenge smoke reached 67% (4/6; failed cases `all_tests_pass`, `has_tests`), so it is non-regressing source-patch evidence only. The saved-artifact replay is full-passing support evidence, not proof of autonomous Senior SWE-Bench performance.
